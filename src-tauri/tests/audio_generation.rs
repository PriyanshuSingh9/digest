use digest_lib::{
    ArticleIngestionService, AudioCancellation, AudioError, AudioGenerationRequest,
    AudioGenerationService, AudioProvider, AudioRequest, DigestService, DigestTools,
    NarrationImportance, NarrationIntent, NarrationSegmentDraft, PresentationType, ProvenanceKind,
    SourceCoverageDecision, SourceCoverageTreatment, WriteNarrationPlanInput,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct FakeAudioProvider {
    calls: Arc<AtomicUsize>,
}

impl AudioProvider for FakeAudioProvider {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn mime_type(&self) -> &'static str {
        "audio/wav"
    }

    fn default_voice(&self) -> &'static str {
        "test-voice"
    }

    fn synthesize<'a>(
        &'a self,
        _request: AudioRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AudioError>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(one_second_wav()) })
    }
}

#[tokio::test]
async fn generates_timed_segment_audio_and_reuses_cached_audio() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let article = ArticleIngestionService::new(service.clone())
        .persist_response(
            "job-1",
            "https://example.com/article",
            "https://example.com/article",
            200,
            Some("text/html"),
            b"<article><h1>Queues</h1><p>A durable queue separates producers from consumers.</p></article>",
        )
        .expect("persist normalized article")
        .article;
    DigestTools::new(service.clone())
        .write_narration_plan(WriteNarrationPlanInput {
            job_id: "job-1".into(),
            article_id: article.artifact_id,
            title: "Queues".into(),
            segments: vec![NarrationSegmentDraft {
                display_text: "A durable queue separates producers from consumers.".into(),
                tts_text: "A durable queue separates producers from consumers.".into(),
                source_blocks: vec!["block-2".into()],
                presentation_type: PresentationType::ConceptCard,
                importance: NarrationImportance::Core,
                intent: NarrationIntent::Explanation,
                provenance: ProvenanceKind::SourceDerived,
            }],
            source_coverage_decisions: vec![
                SourceCoverageDecision {
                    source_blocks: vec!["block-1".into()],
                    treatment: SourceCoverageTreatment::Skip,
                    rationale: "Heading is represented by the lesson title.".into(),
                },
                SourceCoverageDecision {
                    source_blocks: vec!["block-2".into()],
                    treatment: SourceCoverageTreatment::Teach,
                    rationale: "The article's central concept.".into(),
                },
            ],
        })
        .expect("persist narration plan");
    let calls = Arc::new(AtomicUsize::new(0));
    let generator = AudioGenerationService::new(
        service.clone(),
        FakeAudioProvider {
            calls: calls.clone(),
        },
    );

    let request = AudioGenerationRequest {
        voice: "test-voice".into(),
        speed: 1.0,
        regenerate_segment_ids: Default::default(),
    };
    let first = generator
        .generate("job-1", &request, &AudioCancellation::new(), |_| {})
        .await
        .expect("generate audio");
    assert_eq!(first.generated_segment_count, 1);
    assert_eq!(first.reused_segment_count, 0);
    assert_eq!(first.manifest.payload["audio"]["durationMs"], 1_000);
    assert_eq!(first.manifest.payload["segments"][0]["startMs"], 0);
    assert_eq!(first.manifest.payload["segments"][0]["endMs"], 1_000);
    let audio_id = first.manifest.payload["segments"][0]["parts"][0]["audioArtifactId"]
        .as_str()
        .expect("audio artifact ID");
    assert_eq!(
        service
            .read_binary_artifact(audio_id)
            .expect("read generated audio"),
        one_second_wav()
    );

    let second = generator
        .generate("job-1", &request, &AudioCancellation::new(), |_| {})
        .await
        .expect("reuse audio");
    assert_eq!(second.generated_segment_count, 0);
    assert_eq!(second.reused_segment_count, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
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
