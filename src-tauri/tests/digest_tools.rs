use digest_lib::{
    AnalysisClaim, AnalysisFinding, AnalysisFindingKind, ArticleIngestionService, DiagramEdge,
    DiagramNode, DigestService, DigestTools, NarrationImportance, NarrationIntent,
    NarrationSegmentDraft, PresentationType, ProvenanceKind, ReadArtifactInput,
    SourceCoverageDecision, SourceCoverageTreatment, VisualSpec, WriteAnalysisInput,
    WriteNarrationPlanInput,
};
use std::sync::Arc;

fn coverage_decision(
    source_blocks: &[&str],
    treatment: SourceCoverageTreatment,
) -> SourceCoverageDecision {
    SourceCoverageDecision {
        source_blocks: source_blocks.iter().map(|block| (*block).into()).collect(),
        treatment,
        rationale: "Explicit test coverage decision.".into(),
    }
}

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
                kind: ProvenanceKind::SourceDerived,
            },
            findings: vec![AnalysisFinding {
                category: AnalysisFindingKind::VisualizationOpportunity,
                text: "Independent components can tolerate load spikes.".into(),
                source_blocks: vec!["block-2".into()],
                kind: ProvenanceKind::AiInference,
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
                kind: ProvenanceKind::SourceDerived,
            },
            findings: vec![AnalysisFinding {
                category: AnalysisFindingKind::KeyClaim,
                text: "A valid point.".into(),
                source_blocks: vec!["block-1".into()],
                kind: ProvenanceKind::SourceDerived,
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
                provenance: ProvenanceKind::SourceDerived,
                image_id: None,
                visual: None,
            }],
            source_coverage_decisions: vec![
                coverage_decision(&["block-1"], SourceCoverageTreatment::Teach),
                coverage_decision(&["block-2"], SourceCoverageTreatment::Skip),
            ],
            image_coverage_decisions: vec![],
        })
        .expect("write narration plan");

    assert_eq!(written.payload["schemaVersion"], "1.5");
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
    assert_eq!(written.payload["diagnostics"]["accountedBlockCount"], 2);
    assert_eq!(written.payload["diagnostics"]["taughtBlockCount"], 1);
    assert_eq!(written.payload["diagnostics"]["skippedBlockCount"], 1);
    assert_eq!(written.payload["diagnostics"]["sourceWordCount"], 6);
    assert_eq!(written.payload["diagnostics"]["narrationWordCount"], 4);
    assert_eq!(
        written.payload["diagnostics"]["narrationToSourceWordPercent"],
        66
    );

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
                provenance: ProvenanceKind::AiExplanation,
                image_id: None,
                visual: None,
            }],
            source_coverage_decisions: vec![coverage_decision(
                &["block-1", "block-2"],
                SourceCoverageTreatment::Skip,
            )],
            image_coverage_decisions: vec![],
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
    assert!(encoded_schema.contains("\"sourceCoverageDecisions\""));
    assert!(
        serde_json::from_value::<WriteNarrationPlanInput>(serde_json::json!({
            "jobId": "job-1",
            "articleId": "article-1",
            "title": "Incomplete plan",
            "segments": []
        }))
        .expect_err("coverage decisions are part of the narration contract")
        .to_string()
        .contains("sourceCoverageDecisions")
    );
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
        }],
        "sourceCoverageDecisions": []
    }))
    .expect_err("unsupported presentation type must fail at the tool boundary");
    assert!(error.to_string().contains("unknown variant"));
    assert!(error.to_string().contains("article-text"));
}

#[test]
fn narration_requires_an_explicit_decision_for_every_source_diagram() {
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
            provenance: ProvenanceKind::SourceDerived,
            image_id: None,
            visual: None,
        }],
        source_coverage_decisions: vec![
            coverage_decision(&["block-1"], SourceCoverageTreatment::Skip),
            coverage_decision(&["block-2"], SourceCoverageTreatment::Summarize),
        ],
        image_coverage_decisions: vec![],
    };

    let error = tools
        .write_narration_plan(text_only.clone())
        .expect_err("a plan must not silently ignore a meaningful diagram");
    assert!(error.to_string().contains("2 of 3 source blocks"));

    let mut diagram_skipped = text_only;
    diagram_skipped
        .source_coverage_decisions
        .push(coverage_decision(
            &["block-3"],
            SourceCoverageTreatment::Skip,
        ));
    let skipped = tools
        .write_narration_plan(diagram_skipped)
        .expect("a diagram may be skipped with an explicit rationale");
    assert_eq!(skipped.payload["diagnostics"]["referencedDiagramCount"], 0);
    assert_eq!(skipped.payload["diagnostics"]["skippedBlockCount"], 2);

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
                provenance: ProvenanceKind::SourceDerived,
                image_id: None,
                visual: Some(VisualSpec::Diagram {
                    nodes: vec![
                        DiagramNode {
                            id: "api".into(),
                            label: "API".into(),
                        },
                        DiagramNode {
                            id: "log".into(),
                            label: "Durable log".into(),
                        },
                        DiagramNode {
                            id: "replica".into(),
                            label: "Replica".into(),
                        },
                    ],
                    edges: vec![
                        DiagramEdge {
                            from: "api".into(),
                            to: "log".into(),
                            label: "appends".into(),
                        },
                        DiagramEdge {
                            from: "log".into(),
                            to: "replica".into(),
                            label: String::new(),
                        },
                    ],
                }),
            }],
            source_coverage_decisions: vec![
                coverage_decision(&["block-1", "block-2"], SourceCoverageTreatment::Skip),
                coverage_decision(&["block-3"], SourceCoverageTreatment::Teach),
            ],
            image_coverage_decisions: vec![],
        })
        .expect("present a source diagram");

    assert_eq!(written.payload["segments"][0]["presentation"]["type"], "diagram");
    assert_eq!(
        written.payload["segments"][0]["presentation"]["visual"]["type"],
        "diagram"
    );
    assert_eq!(
        written.payload["segments"][0]["presentation"]["visual"]["nodes"][0]["id"],
        "api"
    );
    assert_eq!(written.payload["diagnostics"]["diagramBlockCount"], 1);
    assert_eq!(written.payload["diagnostics"]["referencedDiagramCount"], 1);
    assert_eq!(
        written.payload["diagnostics"]["diagramCoveragePercent"],
        100
    );
    assert_eq!(
        written.payload["sourceCoverageDecisions"][1]["treatment"],
        "teach"
    );
}

/// A one-segment plan citing `cited`, which is taught while every other known
/// block is skipped. Keeps the coverage accounting satisfied so the test
/// exercises only the visual rule under test.
fn visual_plan(
    article_id: String,
    presentation_type: PresentationType,
    visual: Option<VisualSpec>,
    cited: &str,
) -> WriteNarrationPlanInput {
    let mut decisions = Vec::new();
    for block in ["block-1", "block-2", "block-3"] {
        let treatment = if block == cited {
            SourceCoverageTreatment::Teach
        } else {
            SourceCoverageTreatment::Skip
        };
        decisions.push(coverage_decision(&[block], treatment));
    }
    WriteNarrationPlanInput {
        job_id: "job-1".into(),
        article_id,
        title: "Visuals".into(),
        segments: vec![NarrationSegmentDraft {
            display_text: "Writes pass through a durable log.".into(),
            tts_text: "Writes pass through a durable log.".into(),
            source_blocks: vec![cited.into()],
            presentation_type,
            importance: NarrationImportance::Core,
            intent: NarrationIntent::Explanation,
            provenance: ProvenanceKind::SourceDerived,
            image_id: None,
            visual,
        }],
        source_coverage_decisions: decisions,
        image_coverage_decisions: vec![],
    }
}

fn diagram_visual() -> VisualSpec {
    VisualSpec::Diagram {
        nodes: vec![
            DiagramNode {
                id: "api".into(),
                label: "API".into(),
            },
            DiagramNode {
                id: "log".into(),
                label: "Durable log".into(),
            },
        ],
        edges: vec![DiagramEdge {
            from: "api".into(),
            to: "log".into(),
            label: String::new(),
        }],
    }
}

#[test]
fn narration_visual_specs_are_required_where_drawn_and_forbidden_elsewhere() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let article = ArticleIngestionService::new(service.clone())
        .persist_response(
            "job-1",
            "https://example.com/article",
            "https://example.com/article",
            200,
            Some("text/html"),
            br#"<article><h1>Durable logs</h1><p>Writes pass through a durable log.</p><svg aria-label="Writes flow from the API through the durable log to replicas"><text>API</text><text>Log</text></svg></article>"#,
        )
        .expect("persist article")
        .article;
    let tools = DigestTools::new(service);
    let article_id = article.artifact_id.clone();

    let error = tools
        .write_narration_plan(visual_plan(
            article_id.clone(),
            PresentationType::Diagram,
            None,
            "block-3",
        ))
        .expect_err("diagram without a visual must fail");
    assert!(error.to_string().contains("carries no `diagram` visual"));

    let error = tools
        .write_narration_plan(visual_plan(
            article_id.clone(),
            PresentationType::ConceptCard,
            None,
            "block-2",
        ))
        .expect_err("concept card without points must fail");
    assert!(error.to_string().contains("carries no `points` visual"));

    let error = tools
        .write_narration_plan(visual_plan(
            article_id.clone(),
            PresentationType::ArticleText,
            Some(diagram_visual()),
            "block-2",
        ))
        .expect_err("visual on article text must fail");
    assert!(error.to_string().contains("not `diagram` or `concept-card`"));

    let written = tools
        .write_narration_plan(visual_plan(
            article_id.clone(),
            PresentationType::ConceptCard,
            Some(VisualSpec::Points {
                items: vec!["Log first.".into(), "Replicas follow.".into()],
            }),
            "block-2",
        ))
        .expect("points card must pass");
    assert_eq!(
        written.payload["segments"][0]["presentation"]["visual"]["type"],
        "points"
    );
    assert_eq!(written.payload["schemaVersion"], "1.5");
}

#[test]
fn narration_diagram_edges_must_reference_defined_nodes() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let article = ArticleIngestionService::new(service.clone())
        .persist_response(
            "job-1",
            "https://example.com/article",
            "https://example.com/article",
            200,
            Some("text/html"),
            br#"<article><h1>Durable logs</h1><p>Writes pass through a durable log.</p><svg aria-label="Writes flow from the API through the durable log to replicas"><text>API</text><text>Log</text></svg></article>"#,
        )
        .expect("persist article")
        .article;
    let tools = DigestTools::new(service);

    let error = tools
        .write_narration_plan(visual_plan(
            article.artifact_id,
            PresentationType::Diagram,
            Some(VisualSpec::Diagram {
                nodes: vec![
                    DiagramNode {
                        id: "api".into(),
                        label: "API".into(),
                    },
                    DiagramNode {
                        id: "log".into(),
                        label: "Durable log".into(),
                    },
                ],
                edges: vec![DiagramEdge {
                    from: "api".into(),
                    to: "ghost".into(),
                    label: String::new(),
                }],
            }),
            "block-3",
        ))
        .expect_err("dangling edge must fail");
    let message = error.to_string();
    assert!(message.contains("undefined node"), "{message}");
    assert!(message.contains("ghost"), "{message}");
}

#[test]
fn narration_plans_ignore_images_without_bytes() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let article = ArticleIngestionService::new(service.clone())
        .persist_response(
            "job-1",
            "https://example.com/article",
            "https://example.com/article",
            200,
            Some("text/html"),
            b"<article><h1>Durable logs</h1><p>Writes pass through a durable log.</p><img src=\"http://127.0.0.1:9/unreachable.png\" alt=\"Unreachable figure\" width=\"800\" height=\"400\"></article>",
        )
        .expect("persist article")
        .article;
    assert!(
        article.payload["images"]
            .as_array()
            .expect("images array")
            .iter()
            .all(|image| image["captureStatus"] != "localized"),
        "unlocalized images carry no bytes to show"
    );
    let tools = DigestTools::new(service);

    let written = tools
        .write_narration_plan(WriteNarrationPlanInput {
            job_id: "job-1".into(),
            article_id: article.artifact_id,
            title: "Durable logs".into(),
            segments: vec![NarrationSegmentDraft {
                display_text: "Writes pass through a durable log.".into(),
                tts_text: "Writes pass through a durable log.".into(),
                source_blocks: vec!["block-2".into()],
                presentation_type: PresentationType::ArticleText,
                importance: NarrationImportance::Core,
                intent: NarrationIntent::Explanation,
                provenance: ProvenanceKind::SourceDerived,
                visual: None,
                image_id: None,
            }],
            source_coverage_decisions: vec![
                coverage_decision(&["block-1"], SourceCoverageTreatment::Skip),
                coverage_decision(&["block-2"], SourceCoverageTreatment::Teach),
            ],
            image_coverage_decisions: vec![],
        })
        .expect("failed images need no decision");

    assert_eq!(written.payload["schemaVersion"], "1.5");
    assert_eq!(
        written.payload["diagnostics"]["localizedImageCount"],
        0
    );
}

#[test]
fn narration_rejects_tts_text_that_adds_new_explanation() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let article = ArticleIngestionService::new(service.clone())
        .persist_response(
            "job-1",
            "https://example.com/article",
            "https://example.com/article",
            200,
            Some("text/html"),
            b"<article><h1>Durable logs</h1><p>The WAL records every acknowledged write.</p></article>",
        )
        .expect("persist article")
        .article;
    let tools = DigestTools::new(service);

    let error = tools
        .write_narration_plan(WriteNarrationPlanInput {
            job_id: "job-1".into(),
            article_id: article.artifact_id,
            title: "Durable logs".into(),
            segments: vec![NarrationSegmentDraft {
                display_text: "The WAL records every acknowledged write.".into(),
                tts_text: "The W A L records every acknowledged write. This architecture also makes every replica independently scalable and easier to operate.".into(),
                source_blocks: vec!["block-2".into()],
                presentation_type: PresentationType::ArticleText,
                importance: NarrationImportance::Core,
                intent: NarrationIntent::Explanation,
                provenance: ProvenanceKind::SourceDerived,
                image_id: None,
                visual: None,
            }],
            source_coverage_decisions: vec![
                coverage_decision(&["block-1"], SourceCoverageTreatment::Skip),
                coverage_decision(&["block-2"], SourceCoverageTreatment::Teach),
            ],
            image_coverage_decisions: vec![],
        })
        .expect_err("ttsText must not add educational content");

    assert!(error.to_string().contains("pronunciation"));
}

/// The self-correcting-error contract, exercised through the same argument-level entry points the
/// MCP tool handlers use.
mod self_correcting_errors {
    use digest_lib::{ArticleIngestionService, DigestMcpServer, DigestService, DigestTools};
    use rmcp::ErrorData;
    use serde_json::{json, Value};
    use std::sync::Arc;

    const INVALID_PARAMS: i32 = -32602;
    const INTERNAL_ERROR: i32 = -32603;

    const ARTICLE: &[u8] =
        b"<article><h1>Durable logs</h1><p>The WAL records every acknowledged write.</p></article>";

    struct Fixture {
        _directory: tempfile::TempDir,
        server: DigestMcpServer,
        article_id: String,
    }

    fn fixture() -> Fixture {
        let directory = tempfile::tempdir().expect("create temporary data directory");
        let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
        let article = ArticleIngestionService::new(service.clone())
            .persist_response(
                "job-1",
                "https://example.com/article",
                "https://example.com/article",
                200,
                Some("text/html"),
                ARTICLE,
            )
            .expect("persist normalized article")
            .article;
        let server = DigestMcpServer::new(DigestTools::new(service));
        Fixture {
            _directory: directory,
            server,
            article_id: article.artifact_id,
        }
    }

    /// Every agent-correctable failure must arrive as invalid_params, never as a server fault.
    fn assert_reported_as_invalid_params(error: &ErrorData) {
        assert_eq!(error.code.0, INVALID_PARAMS, "message: {}", error.message);
        assert_ne!(error.code.0, INTERNAL_ERROR);
    }

    fn valid_analysis(fixture: &Fixture) -> Value {
        json!({
            "jobId": "job-1",
            "articleId": fixture.article_id,
            "centralArgument": {
                "text": "The write-ahead log records every acknowledged write.",
                "sourceBlocks": ["block-2"],
                "kind": "source_derived"
            },
            "findings": [{
                "category": "key_claim",
                "text": "Acknowledged writes are recorded before they are applied.",
                "sourceBlocks": ["block-2"],
                "kind": "source_derived"
            }]
        })
    }

    fn segment(display_text: &str, tts_text: &str, source_blocks: &[&str]) -> Value {
        json!({
            "displayText": display_text,
            "ttsText": tts_text,
            "sourceBlocks": source_blocks,
            "presentationType": "article-text",
            "importance": "core",
            "intent": "explanation",
            "provenance": "source_derived"
        })
    }

    fn plan(fixture: &Fixture, segments: Value) -> Value {
        json!({
            "jobId": "job-1",
            "articleId": fixture.article_id,
            "title": "Durable logs",
            "segments": segments,
            "sourceCoverageDecisions": [
                { "sourceBlocks": ["block-1"], "treatment": "skip", "rationale": "Title only." },
                { "sourceBlocks": ["block-2"], "treatment": "teach", "rationale": "Core claim." }
            ],
            "imageCoverageDecisions": []
        })
    }

    #[test]
    fn a_well_formed_call_still_succeeds_through_the_owned_decode() {
        let fixture = fixture();
        let written = fixture
            .server
            .write_analysis_arguments(&valid_analysis(&fixture))
            .expect("a valid analysis is accepted");
        assert!(!written.artifact_id.is_empty());
        assert!(!written.content_hash.is_empty());

        let read = fixture
            .server
            .read_artifact_arguments(&json!({ "artifactId": written.artifact_id }))
            .expect("the persisted artifact is readable");
        assert_eq!(read.artifact.payload["schemaVersion"], "1.1");
    }

    #[test]
    fn a_provenance_value_in_the_category_field_names_both_vocabularies() {
        let fixture = fixture();
        let mut arguments = valid_analysis(&fixture);
        arguments["findings"][0]["category"] = json!("ai_explanation");
        let error = fixture
            .server
            .write_analysis_arguments(&arguments)
            .expect_err("ai_explanation is a ProvenanceKind, not an AnalysisFindingKind");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(message.contains("findings[0].category"), "{message}");
        assert!(message.contains("AnalysisFindingKind"), "{message}");
        assert!(message.contains("never takes ProvenanceKind"), "{message}");
        assert!(
            message.contains("Allowed: major_concept, supporting_concept, key_claim"),
            "{message}"
        );
        assert!(
            message.contains("\"category\": \"major_concept\""),
            "{message}"
        );
    }

    #[test]
    fn a_narration_intent_in_the_presentation_type_field_names_both_vocabularies() {
        let fixture = fixture();
        let mut arguments = plan(
            &fixture,
            json!([segment(
                "The log records every acknowledged write.",
                "The log records every acknowledged write.",
                &["block-2"]
            )]),
        );
        arguments["segments"][0]["presentationType"] = json!("quantification");
        let error = fixture
            .server
            .write_narration_plan_arguments(&arguments)
            .expect_err("quantification is a NarrationIntent, not a PresentationType");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(
            message.contains("segments[0].presentationType"),
            "{message}"
        );
        assert!(
            message.contains("Allowed: article-text, callout, code, concept-card"),
            "{message}"
        );
        assert!(message.contains("never takes NarrationIntent"), "{message}");
    }

    #[test]
    fn a_presentation_type_in_the_intent_field_names_both_vocabularies() {
        let fixture = fixture();
        let mut arguments = plan(
            &fixture,
            json!([segment(
                "The log records every acknowledged write.",
                "The log records every acknowledged write.",
                &["block-2"]
            )]),
        );
        arguments["segments"][0]["intent"] = json!("concept-card");
        let error = fixture
            .server
            .write_narration_plan_arguments(&arguments)
            .expect_err("concept-card is a PresentationType, not a NarrationIntent");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(message.contains("segments[0].intent"), "{message}");
        assert!(
            message.contains("Allowed: introduction, explanation, example, comparison"),
            "{message}"
        );
        assert!(
            message.contains("never takes PresentationType"),
            "{message}"
        );
    }

    #[test]
    fn a_double_encoded_claim_shows_the_uncoded_object_and_the_corrected_call() {
        let fixture = fixture();
        let mut arguments = valid_analysis(&fixture);
        let claim = arguments["centralArgument"].clone();
        arguments["centralArgument"] = json!(claim.to_string());
        let error = fixture
            .server
            .write_analysis_arguments(&arguments)
            .expect_err("a string that contains the claim object is double encoded");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(message.contains("centralArgument"), "{message}");
        assert!(message.contains("serialized twice"), "{message}");
        assert!(
            message.contains(r#""sourceBlocks": ["block-1", "block-2"]"#),
            "{message}"
        );
        assert!(message.contains("Corrected call: {"), "{message}");
    }

    #[test]
    fn a_missing_finding_field_reports_the_exact_path_and_an_example() {
        let fixture = fixture();
        let mut arguments = valid_analysis(&fixture);
        arguments["findings"][0]
            .as_object_mut()
            .expect("finding object")
            .remove("text");
        let error = fixture
            .server
            .write_analysis_arguments(&arguments)
            .expect_err("a finding without text cannot be grounded");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(
            message.contains("missing required field `findings[0].text`"),
            "{message}"
        );
        assert!(
            message.contains("Received keys: category, kind, sourceBlocks"),
            "{message}"
        );
        assert!(message.contains(r#""category": "key_claim""#), "{message}");
    }

    #[test]
    fn an_object_where_an_array_belongs_reports_the_field_and_an_example() {
        let fixture = fixture();
        let mut arguments = valid_analysis(&fixture);
        arguments["findings"] = json!({ "0": arguments["findings"][0].clone() });
        let error = fixture
            .server
            .write_analysis_arguments(&arguments)
            .expect_err("findings is a sequence, not a map");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(message.contains("findings must be"), "{message}");
        assert!(
            message.contains("a non-empty array of AnalysisFinding objects"),
            "{message}"
        );
        assert!(message.contains(r#""category": "key_claim""#), "{message}");
    }

    #[test]
    fn a_bare_findings_list_reports_the_expected_top_level_object() {
        let fixture = fixture();
        let bare = json!([valid_analysis(&fixture)["findings"][0].clone()]);
        let error = fixture
            .server
            .write_analysis_arguments(&bare)
            .expect_err("a bare list is not the arguments object");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(message.contains("must be one JSON object"), "{message}");
        assert!(
            message.contains("jobId, articleId, centralArgument, findings"),
            "{message}"
        );
    }

    #[test]
    fn missing_top_level_fields_are_named_instead_of_merely_counted() {
        let fixture = fixture();
        let error = fixture
            .server
            .write_narration_plan_arguments(&json!({
                "jobId": "job-1",
                "articleId": fixture.article_id,
                "segments": [segment(
                    "The log records every acknowledged write.",
                    "The log records every acknowledged write.",
                    &["block-2"]
                )]
            }))
            .expect_err("title and sourceCoverageDecisions are required");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(
            message.contains("missing required top-level field(s): title, sourceCoverageDecisions"),
            "{message}"
        );
        assert!(
            message.contains("Received keys: jobId, articleId, segments"),
            "{message}"
        );
    }

    #[test]
    fn empty_top_level_fields_are_named_against_the_full_required_set() {
        let fixture = fixture();
        let mut arguments = plan(
            &fixture,
            json!([segment(
                "The log records every acknowledged write.",
                "The log records every acknowledged write.",
                &["block-2"]
            )]),
        );
        arguments["title"] = json!("");
        arguments["segments"] = json!([]);
        let error = fixture
            .server
            .write_narration_plan_arguments(&arguments)
            .expect_err("an empty title and an empty segment list are rejected");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(
            message.contains("narration plan requires jobId, articleId, title, and segments"),
            "{message}"
        );
        assert!(
            message.contains("missing or empty: title, segments"),
            "{message}"
        );
    }

    #[test]
    fn an_incomplete_segment_names_the_empty_field() {
        let fixture = fixture();
        let arguments = plan(
            &fixture,
            json!([segment("", "The log records every write.", &["block-2"])]),
        );
        let error = fixture
            .server
            .write_narration_plan_arguments(&arguments)
            .expect_err("a segment without display text cannot be spoken");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(
            message.contains("narration segment 1 is incomplete"),
            "{message}"
        );
        assert!(
            message.contains("displayText (empty or whitespace only)"),
            "{message}"
        );
        assert!(
            message.contains(r#""displayText": "io_uring submits work asynchronously.""#),
            "{message}"
        );
    }

    #[test]
    fn unfaithful_tts_text_reports_the_measured_distance_and_both_examples() {
        let fixture = fixture();
        let arguments = plan(
            &fixture,
            json!([segment(
                "The WAL records every acknowledged write.",
                "The W A L records every acknowledged write. This architecture also makes every \
                 replica independently scalable and easier to operate.",
                &["block-2"]
            )]),
        );
        let error = fixture
            .server
            .write_narration_plan_arguments(&arguments)
            .expect_err("ttsText must not add educational content");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(message.contains("narration segment 1 ttsText"), "{message}");
        assert!(
            message.contains("may only normalize pronunciation"),
            "{message}"
        );
        assert!(message.contains("token edit distance"), "{message}");
        assert!(message.contains("allowed budget 5"), "{message}");
        assert!(message.contains("Accepted normalization:"), "{message}");
        assert!(
            message.contains("displayText \"io_uring submits work asynchronously.\""),
            "{message}"
        );
        assert!(message.contains("Rejected rewrite:"), "{message}");
        assert!(
            message.contains("adds a sentence that displayText does not contain"),
            "{message}"
        );
    }

    #[test]
    fn an_unknown_source_block_is_reported_with_a_sample_of_valid_ids() {
        let fixture = fixture();
        let arguments = plan(
            &fixture,
            json!([segment(
                "The log records every acknowledged write.",
                "The log records every acknowledged write.",
                &["block-2", "block-99"]
            )]),
        );
        let error = fixture
            .server
            .write_narration_plan_arguments(&arguments)
            .expect_err("an invented block id cannot be grounded");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(message.contains("segments[0].sourceBlocks"), "{message}");
        assert!(message.contains("block-99"), "{message}");
        assert!(
            message.contains("The normalized article has 2: block-1, block-2"),
            "{message}"
        );
    }

    #[test]
    fn a_skipped_block_that_is_cited_states_the_rule_and_both_fixes() {
        let fixture = fixture();
        let arguments = plan(
            &fixture,
            json!([segment(
                "Durable logs record acknowledged writes.",
                "Durable logs record acknowledged writes.",
                &["block-1", "block-2"]
            )]),
        );
        let error = fixture
            .server
            .write_narration_plan_arguments(&arguments)
            .expect_err("a skipped block must not be cited");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(message.contains("is marked skip"), "{message}");
        assert!(
            message.contains("a block marked `skip` must not appear in any segment"),
            "{message}"
        );
        assert!(
            message.contains("changing sourceCoverageDecisions[0].treatment to `teach`"),
            "{message}"
        );
    }

    #[test]
    fn unaccounted_source_blocks_are_listed_with_a_corrected_decision() {
        let fixture = fixture();
        let error = fixture
            .server
            .write_narration_plan_arguments(&json!({
                "jobId": "job-1",
                "articleId": fixture.article_id,
                "title": "Durable logs",
                "segments": [segment(
                    "The log records every acknowledged write.",
                    "The log records every acknowledged write.",
                    &["block-2"]
                )],
                "sourceCoverageDecisions": [
                    {
                        "sourceBlocks": ["block-2"],
                        "treatment": "teach",
                        "rationale": "Core claim."
                    }
                ],
                "imageCoverageDecisions": []
            }))
            .expect_err("every block must be accounted for");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(
            message.contains("account for 1 of 2 source blocks"),
            "{message}"
        );
        assert!(
            message.contains("Unaccounted block(s): block-1"),
            "{message}"
        );
        assert!(message.contains(r#""treatment": "teach""#), "{message}");
    }

    #[test]
    fn an_article_from_another_job_reports_both_artifact_and_job() {
        let fixture = fixture();
        let mut arguments = valid_analysis(&fixture);
        arguments["jobId"] = json!("job-2");
        let error = fixture
            .server
            .write_analysis_arguments(&arguments)
            .expect_err("an analysis must belong to the article's job");

        assert_reported_as_invalid_params(&error);
        let message = error.message.as_ref();
        assert!(
            message.contains("does not reference this job's normalized article"),
            "{message}"
        );
        assert!(message.contains("owned by job \"job-1\""), "{message}");
    }

    #[test]
    fn an_unknown_artifact_id_is_a_parameter_error_not_a_server_fault() {
        let fixture = fixture();
        let error = fixture
            .server
            .read_artifact_arguments(&json!({ "artifactId": "article-does-not-exist" }))
            .expect_err("an unknown artifact id is the caller's mistake");

        assert_reported_as_invalid_params(&error);
        assert!(
            error.message.contains("article-does-not-exist"),
            "{}",
            error.message
        );
    }

    #[test]
    fn the_published_schema_separates_the_two_adjacent_enums() {
        let schema = serde_json::to_value(schemars::schema_for!(digest_lib::AnalysisFinding))
            .expect("serialize finding schema");
        let properties = schema["properties"].as_object().expect("properties");
        let category = properties["category"]["description"]
            .as_str()
            .expect("category is described");
        let kind = properties["kind"]["description"]
            .as_str()
            .expect("kind is described");

        assert!(category.contains("AnalysisFindingKind"), "{category}");
        assert!(category.contains("`key_claim`"), "{category}");
        assert!(
            category.contains("does not take ProvenanceKind values"),
            "{category}"
        );
        assert!(kind.contains("ProvenanceKind"), "{kind}");
        assert!(kind.contains("`source_derived`"), "{kind}");
        assert!(
            kind.contains("does not take AnalysisFindingKind values"),
            "{kind}"
        );
        assert_ne!(category, kind);
    }

    #[test]
    fn the_published_segment_schema_separates_presentation_intent_and_provenance() {
        let schema = serde_json::to_value(schemars::schema_for!(digest_lib::NarrationSegmentDraft))
            .expect("serialize segment schema");
        let properties = schema["properties"].as_object().expect("properties");

        let presentation = properties["presentationType"]["description"]
            .as_str()
            .expect("presentationType is described");
        let intent = properties["intent"]["description"]
            .as_str()
            .expect("intent is described");
        let provenance = properties["provenance"]["description"]
            .as_str()
            .expect("provenance is described");

        for value in ["article-text", "concept-card"] {
            assert!(presentation.contains(value), "{presentation}");
        }
        assert!(
            presentation.contains("belong in `intent`"),
            "{presentation}"
        );
        for value in ["quantification", "takeaway"] {
            assert!(intent.contains(value), "{intent}");
        }
        assert!(intent.contains("belong in `presentationType`"), "{intent}");
        for value in [
            "source_derived",
            "ai_explanation",
            "ai_inference",
            "generated_educational",
        ] {
            assert!(provenance.contains(value), "{provenance}");
        }
        assert!(
            provenance.contains("not AnalysisFindingKind and not NarrationIntent"),
            "{provenance}"
        );
    }

    #[test]
    fn every_enum_variant_is_documented_in_the_published_schema() {
        for schema in [
            serde_json::to_value(schemars::schema_for!(digest_lib::AnalysisFindingKind))
                .expect("serialize kind schema"),
            serde_json::to_value(schemars::schema_for!(digest_lib::ProvenanceKind))
                .expect("serialize provenance schema"),
            serde_json::to_value(schemars::schema_for!(digest_lib::PresentationType))
                .expect("serialize presentation schema"),
            serde_json::to_value(schemars::schema_for!(digest_lib::NarrationIntent))
                .expect("serialize intent schema"),
            serde_json::to_value(schemars::schema_for!(digest_lib::NarrationImportance))
                .expect("serialize importance schema"),
            serde_json::to_value(schemars::schema_for!(digest_lib::SourceCoverageTreatment))
                .expect("serialize treatment schema"),
        ] {
            for variant in schema["oneOf"].as_array().expect("one entry per variant") {
                assert!(
                    variant["description"]
                        .as_str()
                        .is_some_and(|description| !description.is_empty()),
                    "every variant carries a description: {variant}"
                );
            }
        }
    }
}
