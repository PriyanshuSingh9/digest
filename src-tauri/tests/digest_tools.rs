use digest_lib::{
    ArticleIngestionService, DigestService, DigestTools, NarrationSegmentDraft, PresentationType,
    ReadArtifactInput, WriteAnalysisInput, WriteNarrationPlanInput,
};
use std::sync::Arc;

#[test]
fn digest_tools_expose_schema_specific_analysis_write_and_artifact_read() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let tools = DigestTools::new(service);

    let written = tools
        .write_analysis(WriteAnalysisInput {
            job_id: "job-1".into(),
            article_id: "article-1".into(),
            summary: "A tool-written analysis.".into(),
        })
        .expect("write analysis through tool facade");
    let read = tools
        .read_artifact(ReadArtifactInput {
            artifact_id: written.artifact_id.clone(),
        })
        .expect("read analysis through tool facade");

    assert_eq!(read.artifact.artifact_id, written.artifact_id);
    assert_eq!(read.artifact.payload["summary"], "A tool-written analysis.");
}

#[test]
fn narration_plan_preserves_display_and_spoken_text_with_source_provenance() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let article = ArticleIngestionService::new(service.clone())
        .persist_response(
            "job-1",
            "https://example.com/article",
            "https://example.com/article",
            200,
            Some("text/html"),
            b"<article><h1>Understanding io_uring</h1><p>Work is submitted asynchronously.</p></article>",
        )
        .expect("persist normalized article")
        .article;
    let tools = DigestTools::new(service);

    let written = tools
        .write_narration_plan(WriteNarrationPlanInput {
            job_id: "job-1".into(),
            article_id: article.artifact_id.clone(),
            title: "Understanding io_uring".into(),
            segments: vec![NarrationSegmentDraft {
                display_text: "io_uring submits work asynchronously.".into(),
                tts_text: "eye-oh uring submits work asynchronously.".into(),
                source_blocks: vec!["block-1".into()],
                presentation_type: PresentationType::ArticleText,
            }],
        })
        .expect("write narration plan");

    assert_eq!(written.payload["schemaVersion"], "1.0");
    assert_eq!(
        written.payload["segments"][0]["displayText"],
        "io_uring submits work asynchronously."
    );
    assert_eq!(
        written.payload["segments"][0]["ttsText"],
        "eye-oh uring submits work asynchronously."
    );
    assert_eq!(written.payload["segments"][0]["id"], "segment-1");
    assert_eq!(written.payload["segments"][0]["sourceBlocks"][0], "block-1");

    let error = tools
        .write_narration_plan(WriteNarrationPlanInput {
            job_id: "job-1".into(),
            article_id: article.artifact_id,
            title: "Invalid provenance".into(),
            segments: vec![NarrationSegmentDraft {
                display_text: "Invented source.".into(),
                tts_text: "Invented source.".into(),
                source_blocks: vec!["block-99".into()],
                presentation_type: PresentationType::ArticleText,
            }],
        })
        .expect_err("unknown source blocks must be rejected");
    assert!(error.to_string().contains("block-99"));
}

#[test]
fn narration_schema_publishes_supported_presentation_types() {
    let schema = serde_json::to_value(schemars::schema_for!(WriteNarrationPlanInput))
        .expect("serialize narration schema");
    let encoded_schema = schema.to_string();
    assert!(encoded_schema.contains("\"article-text\""));
    assert!(encoded_schema.contains("\"concept-card\""));
    let error = serde_json::from_value::<WriteNarrationPlanInput>(serde_json::json!({
        "jobId": "job-1",
        "articleId": "article-1",
        "title": "Invalid plan",
        "segments": [{
            "displayText": "Display",
            "ttsText": "Spoken",
            "sourceBlocks": ["block-1"],
            "presentationType": "narrative"
        }]
    }))
    .expect_err("unsupported presentation type must fail at the tool boundary");
    assert!(error.to_string().contains("unknown variant"));
    assert!(error.to_string().contains("article-text"));
}
