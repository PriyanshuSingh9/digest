use crate::{application::sha256_hex, ArtifactEnvelope, ArtifactKind, DigestError, DigestService};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use thiserror::Error;

const PLAYBACK_MANIFEST_SCHEMA_VERSION: &str = "1.2";
/// Sentence parts shorter than this merge with the following sentence so
/// abbreviations and terse fragments never become degenerate TTS requests.
const MIN_SENTENCE_PART_CHARS: usize = 40;
/// Ogg Opus granule positions always count 48 kHz samples regardless of the
/// encoder's input sample rate (RFC 7845 section 4).
const OPUS_GRANULE_RATE: u64 = 48_000;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error(transparent)]
    Digest(#[from] DigestError),
    #[error("Kokoro request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Kokoro returned HTTP {status}: {message}")]
    Provider { status: u16, message: String },
    #[error("invalid narration plan: {0}")]
    InvalidPlan(String),
    #[error("invalid WAV response: {0}")]
    InvalidWav(String),
    #[error("invalid Ogg Opus response: {0}")]
    InvalidOggOpus(String),
    #[error("provider returned durable audio in unsupported format {0}")]
    UnsupportedFormat(String),
    #[error(
        "audio generation cancelled after {completed_segments} of {total_segments} segments; \
         completed segments remain cached"
    )]
    Cancelled {
        completed_segments: usize,
        total_segments: usize,
    },
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioRequest {
    pub text: String,
    pub voice: String,
    pub speed: f32,
}

pub trait AudioProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// MIME type of the durable audio container this provider delivers. The
    /// generation service refuses formats it cannot independently time.
    fn mime_type(&self) -> &'static str;

    /// Voice used when the caller does not choose one explicitly.
    fn default_voice(&self) -> &'static str;

    fn synthesize<'a>(
        &'a self,
        request: AudioRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AudioError>> + Send + 'a>>;
}

/// Cooperative cancellation observed by the generation loop at segment
/// boundaries. Segments persisted before cancellation stay cached.
#[derive(Clone, Default)]
pub struct AudioCancellation {
    cancelled: Arc<AtomicBool>,
}

impl AudioCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioGenerationProgress {
    pub total_segments: usize,
    pub completed_segments: usize,
    pub generated_segment_count: usize,
    pub reused_segment_count: usize,
    /// Segment currently being synthesized, absent between segments.
    pub current_segment_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct KokoroProvider {
    client: reqwest::Client,
    endpoint: String,
}

impl KokoroProvider {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: endpoint.into().trim_end_matches('/').to_owned(),
        }
    }
}

impl AudioProvider for KokoroProvider {
    fn name(&self) -> &'static str {
        "kokoro"
    }

    fn mime_type(&self) -> &'static str {
        "audio/ogg"
    }

    fn default_voice(&self) -> &'static str {
        "af_sky"
    }

    fn synthesize<'a>(
        &'a self,
        request: AudioRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AudioError>> + Send + 'a>> {
        Box::pin(async move {
            let response = self
                .client
                .post(format!("{}/v1/audio/speech", self.endpoint))
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_vec(&kokoro_speech_request(&request))?)
                .timeout(Duration::from_secs(300))
                .send()
                .await?;
            read_provider_bytes(response).await
        })
    }
}

fn kokoro_speech_request(request: &AudioRequest) -> Value {
    serde_json::json!({
        "model": "tts-1",
        "input": request.text,
        "voice": request.voice,
        "response_format": "opus",
        "speed": request.speed,
        "stream": false
    })
}

async fn read_provider_bytes(response: reqwest::Response) -> Result<Vec<u8>, AudioError> {
    let status = response.status();
    let bytes = response.bytes().await?;
    if !status.is_success() {
        return Err(AudioError::Provider {
            status: status.as_u16(),
            message: String::from_utf8_lossy(&bytes).chars().take(500).collect(),
        });
    }
    Ok(bytes.to_vec())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NarrationPlan {
    article_id: String,
    title: String,
    segments: Vec<NarrationSegment>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NarrationSegment {
    id: String,
    display_text: String,
    tts_text: String,
    source_blocks: Vec<String>,
    provenance: Value,
    presentation: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateAudioResult {
    pub manifest: ArtifactEnvelope,
    pub generated_segment_count: usize,
    pub reused_segment_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackManifest {
    schema_version: &'static str,
    article_id: String,
    title: String,
    audio: ManifestAudio,
    segments: Vec<ManifestSegment>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAudio {
    provider: String,
    voice: String,
    speed: f32,
    duration_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestSegment {
    id: String,
    start_ms: u64,
    end_ms: u64,
    /// Sentence-boundary transport parts in playback order. Part timing is
    /// the sentence-level alignment layer: contiguous within the segment and
    /// derived from the durable audio bytes.
    parts: Vec<ManifestPart>,
    display_text: String,
    source_blocks: Vec<String>,
    provenance: Value,
    presentation: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestPart {
    start_ms: u64,
    end_ms: u64,
    mime_type: String,
    audio_artifact_id: String,
    text: String,
}

/// Parameters for one audio generation pass. Segments named in
/// `regenerate_segment_ids` bypass the durable cache so their audio is
/// synthesized fresh; everything else reuses cached parts.
#[derive(Clone, Debug)]
pub struct AudioGenerationRequest {
    pub voice: String,
    pub speed: f32,
    pub regenerate_segment_ids: std::collections::HashSet<String>,
}

pub struct AudioGenerationService<P> {
    service: Arc<DigestService>,
    provider: P,
}

impl<P: AudioProvider> AudioGenerationService<P> {
    pub fn new(service: Arc<DigestService>, provider: P) -> Self {
        Self { service, provider }
    }

    pub async fn generate(
        &self,
        job_id: &str,
        request: &AudioGenerationRequest,
        cancellation: &AudioCancellation,
        mut on_progress: impl FnMut(AudioGenerationProgress),
    ) -> Result<GenerateAudioResult, AudioError> {
        let voice = request.voice.as_str();
        let speed = request.speed;
        if voice.trim().is_empty() {
            return Err(AudioError::InvalidPlan("voice is required".into()));
        }
        if !(0.25..=4.0).contains(&speed) {
            return Err(AudioError::InvalidPlan(
                "speed must be between 0.25 and 4.0".into(),
            ));
        }
        let narration = self
            .service
            .latest_artifact(job_id, ArtifactKind::NarrationPlan)?
            .ok_or_else(|| AudioError::InvalidPlan("narration plan not found".into()))?;
        let plan: NarrationPlan = serde_json::from_value(narration.payload)?;
        if plan.segments.is_empty() {
            return Err(AudioError::InvalidPlan(
                "narration plan has no segments".into(),
            ));
        }
        for segment_id in &request.regenerate_segment_ids {
            if !plan.segments.iter().any(|segment| &segment.id == segment_id) {
                return Err(AudioError::InvalidPlan(format!(
                    "segment {segment_id} is not in the narration plan"
                )));
            }
        }

        let existing = self.service.list_artifacts(job_id)?;
        let total_segments = plan.segments.len();
        let mut generated_segment_count = 0;
        let mut reused_segment_count = 0;
        let mut elapsed_ms = 0;
        let mut segments = Vec::with_capacity(total_segments);
        // Parts persisted or reused earlier in this same pass, so a sentence
        // repeated within a lesson is synthesized once.
        let mut session_parts: std::collections::HashMap<String, (String, u64, String)> =
            std::collections::HashMap::new();
        on_progress(AudioGenerationProgress {
            total_segments,
            completed_segments: 0,
            generated_segment_count,
            reused_segment_count,
            current_segment_id: None,
        });
        for (index, segment) in plan.segments.into_iter().enumerate() {
            if segment.tts_text.trim().is_empty() {
                return Err(AudioError::InvalidPlan(format!(
                    "segment {} has no TTS text",
                    segment.id
                )));
            }
            on_progress(AudioGenerationProgress {
                total_segments,
                completed_segments: index,
                generated_segment_count,
                reused_segment_count,
                current_segment_id: Some(segment.id.clone()),
            });
            let force_regeneration = request.regenerate_segment_ids.contains(&segment.id);
            let mut segment_generated = false;
            let mut parts = Vec::new();
            let segment_start_ms = elapsed_ms;
            for part_text in split_into_sentence_parts(&segment.tts_text) {
                if cancellation.is_cancelled() {
                    return Err(AudioError::Cancelled {
                        completed_segments: index,
                        total_segments,
                    });
                }
                let cache_key = audio_cache_key(
                    self.provider.name(),
                    self.provider.mime_type(),
                    voice,
                    speed,
                    &segment.id,
                    &part_text,
                );
                let (artifact_id, duration_ms, mime_type) = match session_parts
                    .get(&cache_key)
                    .cloned()
                {
                    Some(part) => part,
                    None => {
                        let cached = if force_regeneration {
                            None
                        } else {
                            // Newest matching artifact wins so a forced
                            // regeneration from an earlier pass stays in effect.
                            existing.iter().rev().find(|artifact| {
                                artifact.kind == ArtifactKind::AudioSegment
                                    && artifact.payload["cacheKey"].as_str() == Some(&cache_key)
                            })
                        };
                        let part = match cached {
                            Some(artifact) => {
                                let duration_ms =
                                    artifact.payload["durationMs"].as_u64().ok_or_else(|| {
                                        AudioError::InvalidPlan(format!(
                                            "cached audio for {} has no duration",
                                            segment.id
                                        ))
                                    })?;
                                let mime_type = artifact.payload["mimeType"]
                                    .as_str()
                                    .ok_or_else(|| {
                                        AudioError::InvalidPlan(format!(
                                            "cached audio for {} has no MIME type",
                                            segment.id
                                        ))
                                    })?
                                    .to_owned();
                                (artifact.artifact_id.clone(), duration_ms, mime_type)
                            }
                            None => {
                                let bytes = self
                                    .provider
                                    .synthesize(AudioRequest {
                                        text: part_text.clone(),
                                        voice: voice.into(),
                                        speed,
                                    })
                                    .await?;
                                let mime_type = self.provider.mime_type().to_owned();
                                let duration_ms = durable_audio_duration_ms(&mime_type, &bytes)?;
                                let payload = serde_json::json!({
                                    "schemaVersion": "1.1",
                                    "narrationArtifactId": narration.artifact_id,
                                    "segmentId": segment.id,
                                    "cacheKey": cache_key,
                                    "provider": self.provider.name(),
                                    "voice": voice,
                                    "speed": speed,
                                    "mimeType": mime_type,
                                    "durationMs": duration_ms,
                                    "byteLength": bytes.len(),
                                    "ttsText": part_text,
                                });
                                segment_generated = true;
                                let artifact = self.service.persist_binary_artifact(
                                    job_id,
                                    ArtifactKind::AudioSegment,
                                    &bytes,
                                    payload,
                                )?;
                                (artifact.artifact_id, duration_ms, mime_type)
                            }
                        };
                        session_parts.insert(cache_key, part.clone());
                        part
                    }
                };
                let start_ms = elapsed_ms;
                elapsed_ms += duration_ms;
                parts.push(ManifestPart {
                    start_ms,
                    end_ms: elapsed_ms,
                    mime_type,
                    audio_artifact_id: artifact_id,
                    text: part_text,
                });
            }
            if segment_generated {
                generated_segment_count += 1;
            } else {
                reused_segment_count += 1;
            }
            on_progress(AudioGenerationProgress {
                total_segments,
                completed_segments: index + 1,
                generated_segment_count,
                reused_segment_count,
                current_segment_id: None,
            });
            segments.push(ManifestSegment {
                id: segment.id,
                start_ms: segment_start_ms,
                end_ms: elapsed_ms,
                parts,
                display_text: segment.display_text,
                source_blocks: segment.source_blocks,
                provenance: segment.provenance,
                presentation: segment.presentation,
            });
        }

        let payload = serde_json::to_value(PlaybackManifest {
            schema_version: PLAYBACK_MANIFEST_SCHEMA_VERSION,
            article_id: plan.article_id,
            title: plan.title,
            audio: ManifestAudio {
                provider: self.provider.name().into(),
                voice: voice.into(),
                speed,
                duration_ms: elapsed_ms,
            },
            segments,
        })?;
        let manifest =
            self.service
                .persist_json_artifact(job_id, ArtifactKind::PlaybackManifest, payload)?;
        Ok(GenerateAudioResult {
            manifest,
            generated_segment_count,
            reused_segment_count,
        })
    }
}

/// Splits narration TTS text into sentence-sized transport parts. This is
/// transport subdivision for generation, seeking, alignment, and partial
/// regeneration — never a content limit: the concatenated parts preserve the
/// complete text. Sentence ends are `.`, `!`, or `?` (plus trailing closing
/// quotes or brackets) followed by whitespace; short fragments merge forward.
pub fn split_into_sentence_parts(text: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character.is_whitespace() {
            if !current.is_empty() && !current.ends_with(' ') {
                current.push(' ');
            }
            continue;
        }
        current.push(character);
        if matches!(character, '.' | '!' | '?') {
            while matches!(
                characters.peek(),
                Some('"' | '\'' | ')' | ']' | '\u{201d}' | '\u{2019}')
            ) {
                current.push(characters.next().expect("peeked closing character"));
            }
            let at_boundary = match characters.peek() {
                Some(next) => next.is_whitespace(),
                None => true,
            };
            if at_boundary && current.trim().chars().count() >= MIN_SENTENCE_PART_CHARS {
                parts.push(current.trim().to_owned());
                current.clear();
            }
        }
    }
    let remainder = current.trim();
    if !remainder.is_empty() {
        // A short trailing fragment merges backward instead of becoming a
        // degenerate synthesis request.
        if remainder.chars().count() < MIN_SENTENCE_PART_CHARS {
            match parts.last_mut() {
                Some(last) => {
                    last.push(' ');
                    last.push_str(remainder);
                }
                None => parts.push(remainder.to_owned()),
            }
        } else {
            parts.push(remainder.to_owned());
        }
    }
    parts
}

fn audio_cache_key(
    provider: &str,
    mime_type: &str,
    voice: &str,
    speed: f32,
    segment_id: &str,
    text: &str,
) -> String {
    sha256_hex(format!("{provider}\0{mime_type}\0{voice}\0{speed}\0{segment_id}\0{text}").as_bytes())
}

/// Derives a segment duration from the durable container bytes themselves so
/// the playback manifest never trusts a provider-reported number. Formats the
/// service cannot time are rejected before anything is persisted.
pub fn durable_audio_duration_ms(mime_type: &str, bytes: &[u8]) -> Result<u64, AudioError> {
    match mime_type {
        "audio/wav" => wav_duration_ms(bytes),
        "audio/ogg" | "audio/opus" => ogg_opus_duration_ms(bytes),
        other => Err(AudioError::UnsupportedFormat(other.into())),
    }
}

/// Reads the duration of a single-stream Ogg Opus file per RFC 7845: the
/// final page's granule position minus the `OpusHead` pre-skip, in 48 kHz
/// samples. Requires an explicit end-of-stream page so truncated downloads
/// cannot silently shorten a lesson.
pub fn ogg_opus_duration_ms(bytes: &[u8]) -> Result<u64, AudioError> {
    let invalid = |message: &str| AudioError::InvalidOggOpus(message.into());
    let mut cursor = 0usize;
    let mut serial = None;
    let mut pre_skip = None;
    let mut last_granule = None;
    let mut end_of_stream = false;
    while cursor < bytes.len() {
        if end_of_stream {
            return Err(invalid("data after end-of-stream page"));
        }
        let header = bytes
            .get(cursor..cursor + 27)
            .ok_or_else(|| invalid("truncated page header"))?;
        if &header[0..4] != b"OggS" {
            return Err(invalid("missing OggS capture pattern"));
        }
        if header[4] != 0 {
            return Err(invalid("unsupported Ogg version"));
        }
        let header_type = header[5];
        let granule = u64::from_le_bytes(header[6..14].try_into().expect("eight granule bytes"));
        let page_serial = u32::from_le_bytes(header[14..18].try_into().expect("four serial bytes"));
        match serial {
            None => serial = Some(page_serial),
            Some(serial) if serial != page_serial => {
                return Err(invalid("multiplexed Ogg streams are not supported"));
            }
            Some(_) => {}
        }
        let segment_count = header[26] as usize;
        let table = bytes
            .get(cursor + 27..cursor + 27 + segment_count)
            .ok_or_else(|| invalid("truncated segment table"))?;
        let body_length: usize = table.iter().map(|length| *length as usize).sum();
        let body_start = cursor + 27 + segment_count;
        let body = bytes
            .get(body_start..body_start + body_length)
            .ok_or_else(|| invalid("truncated page body"))?;
        if pre_skip.is_none() {
            if body.len() < 19 || &body[0..8] != b"OpusHead" {
                return Err(invalid("first packet is not OpusHead"));
            }
            pre_skip = Some(u64::from(u16::from_le_bytes([body[10], body[11]])));
        } else if granule != u64::MAX {
            last_granule = Some(granule);
        }
        if header_type & 0x04 != 0 {
            end_of_stream = true;
        }
        cursor = body_start + body_length;
    }
    if !end_of_stream {
        return Err(invalid("missing end-of-stream page"));
    }
    let pre_skip = pre_skip.ok_or_else(|| invalid("missing OpusHead"))?;
    let last_granule = last_granule.ok_or_else(|| invalid("no audio pages"))?;
    let samples = last_granule
        .checked_sub(pre_skip)
        .ok_or_else(|| invalid("final granule position precedes pre-skip"))?;
    Ok(samples.saturating_mul(1000) / OPUS_GRANULE_RATE)
}

pub fn wav_duration_ms(bytes: &[u8]) -> Result<u64, AudioError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(AudioError::InvalidWav("missing RIFF/WAVE header".into()));
    }
    let mut cursor = 12;
    let mut byte_rate = None;
    let mut data_length = None;
    while cursor + 8 <= bytes.len() {
        let chunk_id = &bytes[cursor..cursor + 4];
        let declared_length = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap());
        let body = cursor + 8;
        if chunk_id == b"data" && declared_length == u32::MAX {
            data_length = Some((bytes.len() - body) as u64);
            break;
        }
        let length = declared_length as usize;
        if body + length > bytes.len() {
            return Err(AudioError::InvalidWav("truncated chunk".into()));
        }
        if chunk_id == b"fmt " {
            if length < 12 {
                return Err(AudioError::InvalidWav("short format chunk".into()));
            }
            byte_rate = Some(u32::from_le_bytes(
                bytes[body + 8..body + 12].try_into().unwrap(),
            ));
        } else if chunk_id == b"data" {
            data_length = Some(length as u64);
        }
        cursor = body + length + (length % 2);
    }
    let byte_rate = byte_rate
        .filter(|rate| *rate > 0)
        .ok_or_else(|| AudioError::InvalidWav("missing byte rate".into()))?;
    let data_length =
        data_length.ok_or_else(|| AudioError::InvalidWav("missing data chunk".into()))?;
    Ok(data_length.saturating_mul(1000) / u64::from(byte_rate))
}

#[cfg(test)]
mod tests {
    use super::{
        durable_audio_duration_ms, kokoro_speech_request, ogg_opus_duration_ms,
        split_into_sentence_parts, wav_duration_ms, AudioCancellation, AudioError,
        AudioGenerationProgress, AudioGenerationRequest, AudioGenerationService, AudioProvider,
        AudioRequest,
    };
    use crate::{ArtifactKind, DigestService};
    use std::{
        collections::VecDeque,
        future::Future,
        pin::Pin,
        sync::{Arc, Mutex},
    };

    const LONG_SENTENCE_1: &str =
        "This first sentence is comfortably longer than the forty character minimum.";
    const LONG_SENTENCE_2: &str =
        "The second sentence also exceeds the configured minimum length easily.";

    fn request(voice: &str) -> AudioGenerationRequest {
        AudioGenerationRequest {
            voice: voice.into(),
            speed: 1.0,
            regenerate_segment_ids: Default::default(),
        }
    }

    const KOKORO_OPUS_PROBE: &[u8] = include_bytes!("../tests/fixtures/kokoro-opus-probe.ogg");

    #[test]
    fn reads_duration_from_pcm_wav_chunks() {
        assert_eq!(wav_duration_ms(&one_second_wav()).unwrap(), 1_000);
    }

    #[test]
    fn rejects_non_wav_provider_responses() {
        assert!(wav_duration_ms(b"{\"error\":\"model unavailable\"}").is_err());
    }

    #[test]
    fn reads_kokoro_wav_with_unknown_stream_length() {
        let mut wav = one_second_wav();
        wav[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        wav[40..44].copy_from_slice(&u32::MAX.to_le_bytes());

        assert_eq!(wav_duration_ms(&wav).unwrap(), 1_000);
    }

    #[test]
    fn reads_rfc7845_duration_from_live_kokoro_opus_fixture() {
        // The fixture was produced by the Kokoros OpenAI endpoint with
        // response_format "opus": pre-skip 156, final granule 138000, so
        // (138000 - 156) / 48 = 2871 ms. ffprobe reports 2.875 s for the
        // same file before pre-skip trimming.
        assert_eq!(ogg_opus_duration_ms(KOKORO_OPUS_PROBE).unwrap(), 2_871);
    }

    #[test]
    fn subtracts_opus_pre_skip_from_final_granule_position() {
        assert_eq!(ogg_opus_duration_ms(&one_second_ogg_opus(312)).unwrap(), 1_000);
    }

    #[test]
    fn rejects_non_ogg_provider_responses() {
        assert!(ogg_opus_duration_ms(b"{\"error\":\"quota exceeded\"}").is_err());
    }

    #[test]
    fn rejects_ogg_stream_without_end_of_stream_page() {
        let mut pages = Vec::new();
        pages.extend_from_slice(&ogg_page(0x02, 0, 0, &opus_head(0)));
        pages.extend_from_slice(&ogg_page(0x00, 48_000, 1, &[0x0b; 40]));

        let error = ogg_opus_duration_ms(&pages).unwrap_err();
        assert!(error.to_string().contains("end-of-stream"), "{error}");
    }

    #[test]
    fn rejects_truncated_ogg_stream() {
        let truncated = &KOKORO_OPUS_PROBE[..KOKORO_OPUS_PROBE.len() - 200];
        assert!(ogg_opus_duration_ms(truncated).is_err());
    }

    #[test]
    fn rejects_durable_formats_the_service_cannot_time() {
        let error = durable_audio_duration_ms("audio/flac", &[0; 16]).unwrap_err();
        assert!(matches!(error, AudioError::UnsupportedFormat(format) if format == "audio/flac"));
    }

    #[test]
    fn kokoro_requests_the_opus_container() {
        let body = kokoro_speech_request(&AudioRequest {
            text: "hello".into(),
            voice: "af_sky".into(),
            speed: 1.0,
        });
        assert_eq!(body["response_format"], "opus");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn splits_narration_text_at_sentence_boundaries() {
        let text = format!("{LONG_SENTENCE_1} {LONG_SENTENCE_2}");
        assert_eq!(
            split_into_sentence_parts(&text),
            vec![LONG_SENTENCE_1.to_owned(), LONG_SENTENCE_2.to_owned()]
        );
    }

    #[test]
    fn short_fragments_merge_instead_of_becoming_parts() {
        // "e.g." style fragments and terse sentences merge forward; a short
        // trailing fragment merges backward.
        let text = format!("E.g. {LONG_SENTENCE_1} {LONG_SENTENCE_2} Yes.");
        assert_eq!(
            split_into_sentence_parts(&text),
            vec![
                format!("E.g. {LONG_SENTENCE_1}"),
                format!("{LONG_SENTENCE_2} Yes."),
            ]
        );
    }

    #[test]
    fn subdivision_preserves_the_complete_text() {
        let text = format!("{LONG_SENTENCE_1}\n\n{LONG_SENTENCE_2} (Really!) Short tail.");
        let parts = split_into_sentence_parts(&text);
        let joined = parts.join(" ");
        let normalized: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(joined.split_whitespace().collect::<Vec<_>>(), normalized);
    }

    #[tokio::test]
    async fn generates_manifest_with_verified_durations_and_mime() {
        let (_directory, service) = service_with_plan(&["s-1", "s-2"]);
        let provider = FakeProvider::new(
            "audio/ogg",
            vec![Ok(one_second_ogg_opus(0)), Ok(one_second_ogg_opus(0))],
        );

        let result = AudioGenerationService::new(Arc::clone(&service), provider)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap();

        assert_eq!(result.generated_segment_count, 2);
        assert_eq!(result.reused_segment_count, 0);
        let manifest = &result.manifest.payload;
        assert_eq!(manifest["schemaVersion"], "1.2");
        assert_eq!(manifest["audio"]["durationMs"], 2_000);
        assert_eq!(manifest["segments"][0]["startMs"], 0);
        assert_eq!(manifest["segments"][0]["endMs"], 1_000);
        assert_eq!(manifest["segments"][0]["parts"][0]["mimeType"], "audio/ogg");
        assert_eq!(manifest["segments"][0]["parts"][0]["startMs"], 0);
        assert_eq!(manifest["segments"][0]["parts"][0]["endMs"], 1_000);
        assert_eq!(manifest["segments"][1]["startMs"], 1_000);
        assert_eq!(manifest["segments"][1]["endMs"], 2_000);
    }

    #[tokio::test]
    async fn long_segments_subdivide_into_contiguous_sentence_parts() {
        let (_directory, service) = service_with_plan_texts(&[(
            "s-1",
            format!("{LONG_SENTENCE_1} {LONG_SENTENCE_2}"),
        )]);
        let provider = FakeProvider::new(
            "audio/ogg",
            vec![Ok(ogg_opus_ms(1_000)), Ok(ogg_opus_ms(500))],
        );

        let result = AudioGenerationService::new(Arc::clone(&service), provider)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap();

        let segment = &result.manifest.payload["segments"][0];
        assert_eq!(segment["startMs"], 0);
        assert_eq!(segment["endMs"], 1_500);
        assert_eq!(segment["parts"][0]["text"], LONG_SENTENCE_1);
        assert_eq!(segment["parts"][0]["startMs"], 0);
        assert_eq!(segment["parts"][0]["endMs"], 1_000);
        assert_eq!(segment["parts"][1]["text"], LONG_SENTENCE_2);
        assert_eq!(segment["parts"][1]["startMs"], 1_000);
        assert_eq!(segment["parts"][1]["endMs"], 1_500);
        assert_ne!(
            segment["parts"][0]["audioArtifactId"],
            segment["parts"][1]["audioArtifactId"]
        );
    }

    #[tokio::test]
    async fn editing_one_sentence_regenerates_only_that_part() {
        let (directory, service) = service_with_plan_texts(&[(
            "s-1",
            format!("{LONG_SENTENCE_1} {LONG_SENTENCE_2}"),
        )]);
        let provider = FakeProvider::new(
            "audio/ogg",
            vec![Ok(ogg_opus_ms(1_000)), Ok(ogg_opus_ms(500))],
        );
        AudioGenerationService::new(Arc::clone(&service), provider)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap();

        let edited = "An edited second sentence that still clears the length minimum fine.";
        persist_plan(&service, &[("s-1", format!("{LONG_SENTENCE_1} {edited}"))]);
        // Exactly one response: only the edited sentence may be synthesized.
        let provider = FakeProvider::new("audio/ogg", vec![Ok(ogg_opus_ms(700))]);
        let result = AudioGenerationService::new(Arc::clone(&service), provider)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap();

        let segment = &result.manifest.payload["segments"][0];
        assert_eq!(segment["parts"][0]["text"], LONG_SENTENCE_1);
        assert_eq!(segment["parts"][1]["text"], edited);
        assert_eq!(segment["parts"][1]["endMs"], 1_700);
        drop(directory);
    }

    #[tokio::test]
    async fn repeated_sentences_synthesize_once_per_pass() {
        let (_directory, service) = service_with_plan_texts(&[(
            "s-1",
            format!("{LONG_SENTENCE_1} {LONG_SENTENCE_1}"),
        )]);
        // Exactly one response: the repeated sentence must reuse it.
        let provider = FakeProvider::new("audio/ogg", vec![Ok(ogg_opus_ms(1_000))]);

        let result = AudioGenerationService::new(Arc::clone(&service), provider)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap();

        let segment = &result.manifest.payload["segments"][0];
        assert_eq!(
            segment["parts"][0]["audioArtifactId"],
            segment["parts"][1]["audioArtifactId"]
        );
        assert_eq!(segment["endMs"], 2_000);
    }

    #[tokio::test]
    async fn forced_regeneration_bypasses_cache_for_named_segments_only() {
        let (_directory, service) = service_with_plan(&["s-1", "s-2"]);
        let provider = FakeProvider::new(
            "audio/ogg",
            vec![Ok(ogg_opus_ms(1_000)), Ok(ogg_opus_ms(1_000))],
        );
        let first = AudioGenerationService::new(Arc::clone(&service), provider)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap();
        let original_artifact =
            first.manifest.payload["segments"][0]["parts"][0]["audioArtifactId"].clone();

        let provider = FakeProvider::new("audio/ogg", vec![Ok(ogg_opus_ms(1_200))]);
        let mut regenerate = request("voice-a");
        regenerate.regenerate_segment_ids.insert("s-1".into());
        let result = AudioGenerationService::new(Arc::clone(&service), provider)
            .generate("job-1", &regenerate, &AudioCancellation::new(), |_| {})
            .await
            .unwrap();

        assert_eq!(result.generated_segment_count, 1);
        assert_eq!(result.reused_segment_count, 1);
        let segment = &result.manifest.payload["segments"][0];
        assert_ne!(segment["parts"][0]["audioArtifactId"], original_artifact);
        assert_eq!(segment["endMs"], 1_200);

        // A later ordinary pass keeps the regenerated audio: newest wins.
        let provider = FakeProvider::new("audio/ogg", vec![]);
        let repeat = AudioGenerationService::new(Arc::clone(&service), provider)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap();
        assert_eq!(
            repeat.manifest.payload["segments"][0]["parts"][0]["audioArtifactId"],
            segment["parts"][0]["audioArtifactId"]
        );
    }

    #[tokio::test]
    async fn regenerating_an_unknown_segment_is_rejected() {
        let (_directory, service) = service_with_plan(&["s-1"]);
        let provider = FakeProvider::new("audio/ogg", vec![]);
        let mut regenerate = request("voice-a");
        regenerate.regenerate_segment_ids.insert("missing".into());

        let error = AudioGenerationService::new(Arc::clone(&service), provider)
            .generate("job-1", &regenerate, &AudioCancellation::new(), |_| {})
            .await
            .unwrap_err();
        assert!(error.to_string().contains("missing"), "{error}");
    }

    #[tokio::test]
    async fn reports_progress_at_segment_boundaries() {
        let (_directory, service) = service_with_plan(&["s-1", "s-2"]);
        let provider = FakeProvider::new(
            "audio/ogg",
            vec![Ok(one_second_ogg_opus(0)), Ok(one_second_ogg_opus(0))],
        );
        let mut progress = Vec::new();

        AudioGenerationService::new(Arc::clone(&service), provider)
            .generate(
                "job-1",
                &request("voice-a"),
                &AudioCancellation::new(),
                |event| {
                    progress.push(event);
                },
            )
            .await
            .unwrap();

        let expected = [
            (0, 0, 0, None),
            (0, 0, 0, Some("s-1")),
            (1, 1, 0, None),
            (1, 1, 0, Some("s-2")),
            (2, 2, 0, None),
        ];
        let observed: Vec<_> = progress
            .iter()
            .map(|event| {
                (
                    event.completed_segments,
                    event.generated_segment_count,
                    event.reused_segment_count,
                    event.current_segment_id.as_deref(),
                )
            })
            .collect();
        assert_eq!(observed, expected);
        assert!(progress.iter().all(|event| event.total_segments == 2));
    }

    #[tokio::test]
    async fn cancellation_at_segment_boundary_preserves_cached_segments() {
        let (_directory, service) = service_with_plan(&["s-1", "s-2"]);
        let provider = FakeProvider::new("audio/ogg", vec![Ok(one_second_ogg_opus(0))]);
        let cancellation = AudioCancellation::new();
        let observer = cancellation.clone();

        let error = AudioGenerationService::new(Arc::clone(&service), provider)
            .generate(
                "job-1",
                &request("voice-a"),
                &cancellation,
                |event: AudioGenerationProgress| {
                    if event.completed_segments == 1 {
                        observer.cancel();
                    }
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            AudioError::Cancelled {
                completed_segments: 1,
                total_segments: 2,
            }
        ));
        assert_eq!(artifact_count(&service, ArtifactKind::AudioSegment), 1);
        assert_eq!(artifact_count(&service, ArtifactKind::PlaybackManifest), 0);
    }

    #[tokio::test]
    async fn provider_failure_preserves_completed_segments_for_retry() {
        let (_directory, service) = service_with_plan(&["s-1", "s-2"]);
        let failing = FakeProvider::new(
            "audio/ogg",
            vec![
                Ok(one_second_ogg_opus(0)),
                Err(AudioError::Provider {
                    status: 503,
                    message: "overloaded".into(),
                }),
            ],
        );

        let error = AudioGenerationService::new(Arc::clone(&service), failing)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap_err();
        assert!(matches!(error, AudioError::Provider { status: 503, .. }));
        assert_eq!(artifact_count(&service, ArtifactKind::AudioSegment), 1);
        assert_eq!(artifact_count(&service, ArtifactKind::PlaybackManifest), 0);

        let retry = FakeProvider::new("audio/ogg", vec![Ok(one_second_ogg_opus(0))]);
        let result = AudioGenerationService::new(Arc::clone(&service), retry)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap();

        assert_eq!(result.reused_segment_count, 1);
        assert_eq!(result.generated_segment_count, 1);
    }

    #[tokio::test]
    async fn different_durable_format_does_not_reuse_cached_segments() {
        let (_directory, service) = service_with_plan(&["s-1"]);
        let wav_provider = FakeProvider::new("audio/wav", vec![Ok(one_second_wav())]);
        AudioGenerationService::new(Arc::clone(&service), wav_provider)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap();

        let opus_provider = FakeProvider::new("audio/ogg", vec![Ok(one_second_ogg_opus(0))]);
        let result = AudioGenerationService::new(Arc::clone(&service), opus_provider)
            .generate("job-1", &request("voice-a"), &AudioCancellation::new(), |_| {})
            .await
            .unwrap();

        assert_eq!(result.generated_segment_count, 1);
        assert_eq!(result.reused_segment_count, 0);
    }

    struct FakeProvider {
        mime_type: &'static str,
        responses: Mutex<VecDeque<Result<Vec<u8>, AudioError>>>,
    }

    impl FakeProvider {
        fn new(mime_type: &'static str, responses: Vec<Result<Vec<u8>, AudioError>>) -> Self {
            Self {
                mime_type,
                responses: Mutex::new(responses.into()),
            }
        }
    }

    impl AudioProvider for FakeProvider {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn mime_type(&self) -> &'static str {
            self.mime_type
        }

        fn default_voice(&self) -> &'static str {
            "voice-a"
        }

        fn synthesize<'a>(
            &'a self,
            _request: AudioRequest,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AudioError>> + Send + 'a>> {
            let response = self
                .responses
                .lock()
                .expect("fake responses lock")
                .pop_front()
                .expect("unexpected synthesize call");
            Box::pin(async move { response })
        }
    }

    fn service_with_plan(segment_ids: &[&str]) -> (tempfile::TempDir, Arc<DigestService>) {
        let segments: Vec<(&str, String)> = segment_ids
            .iter()
            .map(|id| (*id, format!("Spoken text for {id}")))
            .collect();
        service_with_plan_texts(&segments)
    }

    fn service_with_plan_texts(
        segments: &[(&str, String)],
    ) -> (tempfile::TempDir, Arc<DigestService>) {
        let directory = tempfile::tempdir().expect("create temporary data directory");
        let service =
            Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
        persist_plan(&service, segments);
        (directory, service)
    }

    fn persist_plan(service: &DigestService, segments: &[(&str, String)]) {
        let segments: Vec<_> = segments
            .iter()
            .map(|(id, tts_text)| {
                serde_json::json!({
                    "id": id,
                    "displayText": format!("Display for {id}"),
                    "ttsText": tts_text,
                    "sourceBlocks": ["block-1"],
                    "provenance": { "kind": "source" },
                    "presentation": { "type": "text" },
                })
            })
            .collect();
        service
            .persist_json_artifact(
                "job-1",
                ArtifactKind::NarrationPlan,
                serde_json::json!({
                    "articleId": "article-1",
                    "title": "Test article",
                    "segments": segments,
                }),
            )
            .expect("persist narration plan");
    }

    fn artifact_count(service: &DigestService, kind: ArtifactKind) -> usize {
        service
            .list_artifacts("job-1")
            .expect("list artifacts")
            .into_iter()
            .filter(|artifact| artifact.kind == kind)
            .count()
    }

    fn opus_head(pre_skip: u16) -> Vec<u8> {
        let mut head = Vec::new();
        head.extend_from_slice(b"OpusHead");
        head.push(1); // version
        head.push(1); // channels
        head.extend_from_slice(&pre_skip.to_le_bytes());
        head.extend_from_slice(&24_000_u32.to_le_bytes()); // input sample rate
        head.extend_from_slice(&0_u16.to_le_bytes()); // output gain
        head.push(0); // mapping family
        head
    }

    /// Builds a single-stream Ogg Opus file with the given duration and no
    /// encoder pre-skip.
    fn ogg_opus_ms(duration_ms: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&ogg_page(0x02, 0, 0, &opus_head(0)));
        bytes.extend_from_slice(&ogg_page(0x00, 0, 1, b"OpusTagsXXXXXXXX"));
        bytes.extend_from_slice(&ogg_page(0x04, duration_ms * 48, 2, &[0x0b; 40]));
        bytes
    }

    /// Builds a single-stream Ogg Opus file whose audio ends exactly one
    /// second after the encoder pre-skip.
    fn one_second_ogg_opus(pre_skip: u16) -> Vec<u8> {
        let final_granule = 48_000 + u64::from(pre_skip);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&ogg_page(0x02, 0, 0, &opus_head(pre_skip)));
        bytes.extend_from_slice(&ogg_page(0x00, 0, 1, b"OpusTagsXXXXXXXX"));
        bytes.extend_from_slice(&ogg_page(0x04, final_granule, 2, &[0x0b; 40]));
        bytes
    }

    fn ogg_page(header_type: u8, granule: u64, sequence: u32, body: &[u8]) -> Vec<u8> {
        assert!(body.len() < 255, "test pages hold one short packet");
        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.push(0); // version
        page.push(header_type);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&7_u32.to_le_bytes()); // bitstream serial
        page.extend_from_slice(&sequence.to_le_bytes());
        page.extend_from_slice(&0_u32.to_le_bytes()); // checksum, unverified
        page.push(1); // one segment
        page.push(body.len() as u8);
        page.extend_from_slice(body);
        page
    }

    fn one_second_wav() -> Vec<u8> {
        let data_length = 8_000_u32;
        let mut bytes = Vec::with_capacity(44 + data_length as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_length).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&8_000_u32.to_le_bytes());
        bytes.extend_from_slice(&8_000_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&8_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_length.to_le_bytes());
        bytes.resize(44 + data_length as usize, 128);
        bytes
    }
}
