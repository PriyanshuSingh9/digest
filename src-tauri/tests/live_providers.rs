//! Live provider validation. These tests talk to a real TTS server and are
//! ignored by default. Run them explicitly:
//!
//! ```text
//! cargo test --test live_providers -- --ignored
//! ```
//!
//! Kokoro needs a local server (`DIGEST_KOKORO_URL`, default
//! `http://127.0.0.1:3000`).

use digest_lib::{durable_audio_duration_ms, AudioProvider, AudioRequest, KokoroProvider};

#[tokio::test]
#[ignore = "requires a running Kokoro server"]
async fn kokoro_delivers_ogg_opus_the_service_can_time() {
    let endpoint = std::env::var("DIGEST_KOKORO_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:3000".into());
    let provider = KokoroProvider::new(endpoint);
    let bytes = provider
        .synthesize(AudioRequest {
            text: "Digest validates durable opus audio end to end.".into(),
            voice: provider.default_voice().into(),
            speed: 1.0,
        })
        .await
        .expect("synthesize through live Kokoro");
    let duration_ms =
        durable_audio_duration_ms(provider.mime_type(), &bytes).expect("derive durable duration");
    assert!(
        (500..30_000).contains(&duration_ms),
        "implausible duration {duration_ms}ms for a short sentence"
    );
}
