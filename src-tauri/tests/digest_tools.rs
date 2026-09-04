use digest_lib::{
    AnalysisClaim, AnalysisFinding, AnalysisFindingKind, ArticleIngestionService, ClaimKind,
    DigestService, DigestTools, NarrationImportance, NarrationIntent, NarrationSegmentDraft,
    PresentationType, ReadArtifactInput, WriteAnalysisInput, WriteNarrationPlanInput,
};
use std::sync::Arc;

#[test]
fn digest_tools_expose_schema_specific_analysis_write_and_artifact_read() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let article = ArticleIngestionService::new(service.clone())
        .persist_response(
            "job-1",
            "https://example.com/article",
            "https://example.com/article",
            200,
            Some("text/html"),
            b"<article><h1>Durable queues</h1><p>A durable queue separates producers from consumers.</p></article>",
        )
        .expect("persist normalized article")
        .article;
    let tools = DigestTools::new(service);

    let written = tools
        .write_analysis(WriteAnalysisInput {
            job_id: "job-1".into(),
            article_id: article.artifact_id.clone(),
            central_argument: AnalysisClaim {
                text: "Durable queues decouple producers and consumers.".into(),
                source_blocks: vec!["block-2".into()],
                kind: ClaimKind::SourceDerived,
            },
            findings: vec![AnalysisFinding {
                category: AnalysisFindingKind::VisualizationOpportunity,
                text: "Independent components can tolerate load spikes.".into(),
                source_blocks: vec!["block-2".into()],
                kind: ClaimKind::AiInference,
            }],
        })
        .expect("write analysis through tool facade");
    let read = tools
        .read_artifact(ReadArtifactInput {
            artifact_id: written.artifact_id.clone(),
        })
        .expect("read analysis through tool facade");

    assert_eq!(read.artifact.artifact_id, written.artifact_id);
    assert_eq!(read.artifact.payload["schemaVersion"], "1.1");
    assert_eq!(
        read.artifact.payload["centralArgument"]["sourceBlocks"][0],
        "block-2"
    );
    assert_eq!(
        read.artifact.payload["findings"][0]["category"],
        "visualization_opportunity"
    );
    assert_eq!(read.artifact.payload["findings"][0]["kind"], "ai_inference");

    let error = tools
        .write_analysis(WriteAnalysisInput {
            job_id: "job-1".into(),
            article_id: article.artifact_id,
            central_argument: AnalysisClaim {
                text: "An unsupported claim.".into(),
                source_blocks: vec!["block-99".into()],
                kind: ClaimKind::SourceDerived,
            },
            findings: vec![AnalysisFinding {
                category: AnalysisFindingKind::KeyClaim,
                text: "A valid point.".into(),
                source_blocks: vec!["block-1".into()],
                kind: ClaimKind::SourceDerived,
            }],
        })
        .expect_err("unknown analysis source blocks must be rejected");
    assert!(error.to_string().contains("block-99"));
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
                importance: NarrationImportance::Core,
                intent: NarrationIntent::Introduction,
                provenance: ClaimKind::SourceDerived,
            }],
        })
        .expect("write narration plan");

    assert_eq!(written.payload["schemaVersion"], "1.2");
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
    assert_eq!(written.payload["segments"][0]["importance"], "core");
    assert_eq!(written.payload["segments"][0]["intent"], "introduction");
    assert_eq!(
        written.payload["segments"][0]["provenance"]["type"],
        "source_derived"
    );
    assert_eq!(written.payload["diagnostics"]["coreSegmentCount"], 1);

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
                importance: NarrationImportance::Supporting,
                intent: NarrationIntent::Explanation,
                provenance: ClaimKind::AiExplanation,
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
    assert!(encoded_schema.contains("\"quantification\""));
    assert!(encoded_schema.contains("\"provenance\""));
    let error = serde_json::from_value::<WriteNarrationPlanInput>(serde_json::json!({
        "jobId": "job-1",
        "articleId": "article-1",
        "title": "Invalid plan",
        "segments": [{
            "displayText": "Display",
            "ttsText": "Spoken",
            "sourceBlocks": ["block-1"],
            "presentationType": "narrative",
            "importance": "core",
            "intent": "quantification",
            "provenance": "source_derived"
        }]
    }))
    .expect_err("unsupported presentation type must fail at the tool boundary");
    assert!(error.to_string().contains("unknown variant"));
    assert!(error.to_string().contains("article-text"));
}

#[test]
fn narration_requires_meaningful_source_diagrams_to_be_presented() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let article = ArticleIngestionService::new(service.clone())
        .persist_response(
            "job-1",
            "https://example.com/article",
            "https://example.com/article",
            200,
            Some("text/html"),
            br#"<article>
                <h1>Replication</h1>
                <p>Writes flow through a durable log before replicas apply them.</p>
                <svg aria-label="Writes flow from the API through the durable log to replicas">
                  <text>API</text><text>Log</text><text>Replica</text>
                </svg>
            </article>"#,
        )
        .expect("persist article with diagram")
        .article;
    let tools = DigestTools::new(service);
    let text_only = WriteNarrationPlanInput {
        job_id: "job-1".into(),
        article_id: article.artifact_id.clone(),
        title: "Replication".into(),
        segments: vec![NarrationSegmentDraft {
            display_text: "Writes pass through a durable log.".into(),
            tts_text: "Writes pass through a durable log.".into(),
            source_blocks: vec!["block-2".into()],
            presentation_type: PresentationType::ArticleText,
            importance: NarrationImportance::Core,
            intent: NarrationIntent::Explanation,
            provenance: ClaimKind::SourceDerived,
        }],
    };

    let error = tools
        .write_narration_plan(text_only)
        .expect_err("a plan must not ignore all meaningful diagrams");
    assert!(error.to_string().contains("diagram"));

    let written = tools
        .write_narration_plan(WriteNarrationPlanInput {
            job_id: "job-1".into(),
            article_id: article.artifact_id,
            title: "Replication".into(),
            segments: vec![NarrationSegmentDraft {
                display_text: "Follow the write from the API to the log and replica.".into(),
                tts_text: "Follow the write from the A P I to the log and replica.".into(),
                source_blocks: vec!["block-3".into()],
                presentation_type: PresentationType::Diagram,
                importance: NarrationImportance::Core,
                intent: NarrationIntent::Explanation,
                provenance: ClaimKind::SourceDerived,
            }],
        })
        .expect("present a source diagram");

    assert_eq!(written.payload["diagnostics"]["diagramBlockCount"], 1);
    assert_eq!(written.payload["diagnostics"]["referencedDiagramCount"], 1);
    assert_eq!(
        written.payload["diagnostics"]["diagramCoveragePercent"],
        100
    );
}
