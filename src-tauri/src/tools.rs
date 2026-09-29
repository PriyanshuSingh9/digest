use crate::{
    ArticleIngestionService, ArtifactEnvelope, DigestError, DigestService, IngestionError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

/// Where a piece of content came from.
/// This vocabulary is used by `centralArgument.kind`, `findings[].kind`, and `segments[].provenance`.
/// It is not the finding type: `findings[].category` takes AnalysisFindingKind values instead.
/// It is not the rhetorical role either: `segments[].intent` takes NarrationIntent values instead.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    /// Restated from the article, supported by the cited sourceBlocks.
    SourceDerived,
    /// Explanation written to make the cited sourceBlocks easier to follow.
    AiExplanation,
    /// A conclusion drawn from the cited sourceBlocks that the article does not state outright.
    AiInference,
    /// Background or framing that the article does not contain at all.
    GeneratedEducational,
}

impl ProvenanceKind {
    /// Every variant, so error messages can never list a name serde would reject.
    pub const ALL: [Self; 4] = [
        Self::SourceDerived,
        Self::AiExplanation,
        Self::AiInference,
        Self::GeneratedEducational,
    ];
}

/// The claim the article argues, together with the blocks that support it.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisClaim {
    /// The claim itself, as one sentence. Must not be empty.
    pub text: String,
    /// Block ids from the normalized article that support this claim.
    /// Must be a non-empty array of ids taken from the article artifact, for example `block-1`.
    pub source_blocks: Vec<String>,
    /// Provenance of this claim: a ProvenanceKind such as `source_derived`.
    /// Takes the ProvenanceKind vocabulary (`source_derived`, `ai_explanation`, `ai_inference`,
    /// `generated_educational`). It does not take AnalysisFindingKind values.
    pub kind: ProvenanceKind,
}

/// What a finding is in the article.
/// This vocabulary belongs to `findings[].category` only.
/// It is not ProvenanceKind, which belongs to the sibling `kind` field, and it is not NarrationIntent,
/// which belongs to `segments[].intent`. Example: `category` = `key_claim`.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisFindingKind {
    /// An idea the reader cannot follow the article without.
    MajorConcept,
    /// An idea that supports a major concept but is not load-bearing on its own.
    SupportingConcept,
    /// A claim the article asserts and could be argued against.
    KeyClaim,
    /// A worked instance that demonstrates a concept.
    Example,
    /// A term the article pins down.
    Definition,
    /// Two things the article contrasts or measures against each other.
    Comparison,
    /// A stated cause and effect.
    CausalRelationship,
    /// A code snippet the article presents.
    CodeExample,
    /// A measurement, figure, or numeric claim.
    QuantitativeClaim,
    /// A named person, product, protocol, or organization.
    ImportantEntity,
    /// A passage a learner is likely to struggle with.
    DifficultSection,
    /// Knowledge required before the article makes sense.
    Prerequisite,
    /// A place where a diagram, chart, or callout would make the article clearer.
    VisualizationOpportunity,
    /// A place learners are likely to form a wrong mental model.
    PointOfConfusion,
}

impl AnalysisFindingKind {
    /// Every variant, so error messages can never list a name serde would reject.
    pub const ALL: [Self; 14] = [
        Self::MajorConcept,
        Self::SupportingConcept,
        Self::KeyClaim,
        Self::Example,
        Self::Definition,
        Self::Comparison,
        Self::CausalRelationship,
        Self::CodeExample,
        Self::QuantitativeClaim,
        Self::ImportantEntity,
        Self::DifficultSection,
        Self::Prerequisite,
        Self::VisualizationOpportunity,
        Self::PointOfConfusion,
    ];
}

/// One teachable observation about the article, with its supporting blocks.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisFinding {
    /// What this finding is in the article: an AnalysisFindingKind such as `key_claim`.
    /// This field does not take ProvenanceKind values; provenance goes in the sibling `kind` field.
    pub category: AnalysisFindingKind,
    /// The finding, as one sentence. Must not be empty.
    pub text: String,
    /// Block ids from the normalized article that support this finding.
    /// Must be a non-empty array of ids taken from the article artifact, for example `block-1`.
    pub source_blocks: Vec<String>,
    /// Provenance of this finding: a ProvenanceKind such as `source_derived`.
    /// This field does not take AnalysisFindingKind values; the finding type goes in the sibling
    /// `category` field. Passing `major_concept` or `key_claim` here is rejected.
    pub kind: ProvenanceKind,
}

/// Arguments for `write_analysis`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteAnalysisInput {
    /// The running job this analysis belongs to. Must match the normalized article's job.
    pub job_id: String,
    /// Artifact id of the normalized article, as returned by ingest_article in `articleArtifactId`.
    pub article_id: String,
    /// The single claim the article argues, with the blocks that support it.
    /// Required. Pass the object itself, never a string that contains the object.
    pub central_argument: AnalysisClaim,
    /// Non-empty array of AnalysisFinding objects.
    /// Each object needs `category` (AnalysisFindingKind), `text`, `sourceBlocks`, and `kind`
    /// (ProvenanceKind). Pass the array itself, never an object keyed by index.
    pub findings: Vec<AnalysisFinding>,
}

/// Result of `write_analysis`.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteAnalysisOutput {
    /// Id of the persisted analysis artifact.
    pub artifact_id: String,
    /// Content hash of the persisted analysis artifact.
    pub content_hash: String,
}

/// Arguments for `read_artifact`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadArtifactInput {
    /// Id of a persisted artifact, for example the `articleArtifactId` from ingest_article or the
    /// `artifactId` returned by write_analysis.
    pub artifact_id: String,
}

/// Result of `read_artifact`.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadArtifactOutput {
    /// The persisted artifact, including its payload.
    pub artifact: ArtifactEnvelope,
}

/// Arguments for `ingest_article`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngestArticleInput {
    /// The running job to attach the capture to.
    pub job_id: String,
    /// Absolute http or https URL of the article to capture.
    pub url: String,
}

/// Result of `ingest_article`.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngestArticleOutput {
    /// Id of the immutable raw source capture artifact.
    pub source_artifact_id: String,
    /// Id of the normalized article artifact. Use this as `articleId` in the write tools.
    pub article_artifact_id: String,
}

/// One planned narration segment, before ids and diagnostics are assigned.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NarrationSegmentDraft {
    /// The sentence shown on screen. Must not be empty, and it is the reference the spoken text is
    /// measured against.
    pub display_text: String,
    /// The sentence sent to the speech provider. Must not be empty.
    /// It may only normalize pronunciation relative to `displayText` (spelling out an abbreviation,
    /// expanding a symbol, respelling an identifier). It must not add, remove, or paraphrase content.
    pub tts_text: String,
    /// Block ids from the normalized article that this segment presents. Must not be empty.
    pub source_blocks: Vec<String>,
    /// How the segment is drawn on screen. Takes PresentationType in kebab-case:
    /// `article-text`, `callout`, `code`, `concept-card`, `diagram`, `image`, `quote`.
    /// For example `article-text`. Do not put a NarrationIntent value such as `quantification` or
    /// `comparison` here; those belong in `intent`.
    pub presentation_type: PresentationType,
    /// How load-bearing the segment is: `core` or `supporting`.
    pub importance: NarrationImportance,
    /// What the segment does rhetorically. Takes NarrationIntent in snake_case:
    /// `introduction`, `explanation`, `example`, `comparison`, `quantification`, `takeaway`.
    /// For example `explanation`. Do not put a PresentationType value such as `article-text` or
    /// `diagram` here; those belong in `presentationType`.
    pub intent: NarrationIntent,
    /// Provenance of the segment: a ProvenanceKind such as `source_derived`.
    /// Takes the ProvenanceKind vocabulary (`source_derived`, `ai_explanation`, `ai_inference`,
    /// `generated_educational`), not AnalysisFindingKind and not NarrationIntent.
    pub provenance: ProvenanceKind,
}

/// How a narration segment is drawn on screen, in kebab-case.
/// This vocabulary belongs to `segments[].presentationType` only. It is not NarrationIntent, which
/// belongs to `segments[].intent`. Example: `presentationType` = `concept-card`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresentationType {
    /// The article's own prose, shown as prose.
    ArticleText,
    /// A short aside that adds emphasis without changing the source.
    Callout,
    /// A code listing walked through line by line.
    Code,
    /// A self-contained card defining or contrasting one idea.
    ConceptCard,
    /// A source diagram rendered from the article. Requires at least one diagram block in
    /// `sourceBlocks`, otherwise the segment is rejected.
    Diagram,
    /// An image from the article.
    Image,
    /// A verbatim quotation.
    Quote,
}

impl PresentationType {
    /// Every variant, so error messages can never list a name serde would reject.
    pub const ALL: [Self; 7] = [
        Self::ArticleText,
        Self::Callout,
        Self::Code,
        Self::ConceptCard,
        Self::Diagram,
        Self::Image,
        Self::Quote,
    ];
}

/// How load-bearing a narration segment is.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrationImportance {
    /// The learner is lost without this segment.
    Core,
    /// Background that helps but is not required to follow the lesson.
    Supporting,
}

impl NarrationImportance {
    /// Every variant, so error messages can never list a name serde would reject.
    pub const ALL: [Self; 2] = [Self::Core, Self::Supporting];
}

/// What a narration segment does rhetorically, in snake_case.
/// This vocabulary belongs to `segments[].intent` only. It is not PresentationType, which belongs to
/// `segments[].presentationType`. Example: `intent` = `quantification`.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrationIntent {
    /// Opens the lesson and frames the topic.
    Introduction,
    /// Works through the central idea.
    Explanation,
    /// Walks through a concrete case.
    Example,
    /// Sets two things against each other.
    Comparison,
    /// Presents a number, measurement, or figure.
    Quantification,
    /// States what the learner should retain.
    Takeaway,
}

impl NarrationIntent {
    /// Every variant, so error messages can never list a name serde would reject.
    pub const ALL: [Self; 6] = [
        Self::Introduction,
        Self::Explanation,
        Self::Example,
        Self::Comparison,
        Self::Quantification,
        Self::Takeaway,
    ];
}

/// How a source block is accounted for in the plan.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceCoverageTreatment {
    /// The block is taught in its own segment.
    Teach,
    /// The block is folded into a summary of other blocks.
    Summarize,
    /// The block is deliberately not narrated, and must not be cited by any segment.
    Skip,
}

impl SourceCoverageTreatment {
    /// Every variant, so error messages can never list a name serde would reject.
    pub const ALL: [Self; 3] = [Self::Teach, Self::Summarize, Self::Skip];
}

/// An explicit decision covering one or more source blocks.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCoverageDecision {
    /// Block ids this decision covers. Must not be empty, and each block may appear in exactly one
    /// decision. Every block in the article must be accounted for.
    pub source_blocks: Vec<String>,
    /// How these blocks are handled: `teach`, `summarize`, or `skip`.
    /// A `teach` or `summarize` block must be cited by at least one segment; a `skip` block must be
    /// cited by none.
    pub treatment: SourceCoverageTreatment,
    /// Why this treatment was chosen. Must not be empty.
    pub rationale: String,
}

/// Arguments for `write_narration_plan`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteNarrationPlanInput {
    /// The running job this plan belongs to. Must match the normalized article's job.
    pub job_id: String,
    /// Artifact id of the normalized article, as returned by ingest_article in `articleArtifactId`.
    pub article_id: String,
    /// Title of the lesson. Must not be empty.
    pub title: String,
    /// Non-empty, ordered array of narration segment drafts.
    /// Each segment needs `displayText`, `ttsText`, `sourceBlocks`, `presentationType`, `importance`,
    /// `intent`, and `provenance`. Pass the array itself, never an object keyed by index.
    pub segments: Vec<NarrationSegmentDraft>,
    /// Array that accounts for every block in the article exactly once.
    /// Required: omitting it is a parameter error, not a defaulted empty list.
    pub source_coverage_decisions: Vec<SourceCoverageDecision>,
}

/// Upper bound on the cells the reported token edit distance will compute, so a pathological
/// segment cannot make error reporting expensive. Beyond it the message reports lengths only.
const MAX_DISTANCE_CELLS: usize = 250_000;

/// How many block ids an error message quotes before summarizing the rest.
const BLOCK_SAMPLE: usize = 8;

/// An accepted pronunciation normalization, quoted in ttsText faithfulness errors so the agent can
/// see what the rule means in practice.
const ACCEPTED_NORMALIZATION: &str =
    "displayText \"io_uring submits work asynchronously.\" -> ttsText \
     \"eye-oh uring submits work asynchronously.\" (spells the identifier out for the speech \
     provider, edit distance 2)";

/// A rejected rewrite, quoted in ttsText faithfulness errors as the counterexample.
const REJECTED_REWRITE: &str =
    "ttsText \"The W A L records every acknowledged write. This architecture \
     also makes every replica independently scalable.\" for displayText \"The WAL records every \
     acknowledged write.\" (adds a sentence that displayText does not contain)";

#[derive(Clone)]
pub struct DigestTools {
    service: Arc<DigestService>,
}

impl DigestTools {
    pub fn new(service: Arc<DigestService>) -> Self {
        Self { service }
    }

    pub fn write_analysis(
        &self,
        input: WriteAnalysisInput,
    ) -> Result<WriteAnalysisOutput, DigestError> {
        let missing = [
            ("jobId", input.job_id.trim().is_empty()),
            ("articleId", input.article_id.trim().is_empty()),
            ("findings", input.findings.is_empty()),
        ]
        .into_iter()
        .filter_map(|(field, invalid)| invalid.then_some(field))
        .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(DigestError::InvalidInput(format!(
                "write_analysis: analysis requires jobId, articleId, centralArgument, and \
                 findings; missing or empty: {}. `jobId` is the running job, `articleId` is the \
                 normalized article id from ingest_article, `centralArgument` is one AnalysisClaim \
                 object, and `findings` is a non-empty array of AnalysisFinding objects. \
                 Corrected call: {}",
                missing.join(", "),
                crate::tool_input::WRITE_ANALYSIS_EXAMPLE
            )));
        }
        let article = self.service.read_artifact(&input.article_id)?;
        if article.job_id != input.job_id || article.kind != crate::ArtifactKind::NormalizedArticle
        {
            return Err(article_mismatch_error(
                "write_analysis",
                &input.article_id,
                &input.job_id,
                &article,
            ));
        }
        let source_blocks = source_block_ids(&article);
        validate_analysis_claim(
            "write_analysis",
            "centralArgument",
            &input.central_argument,
            &source_blocks,
        )?;
        for (index, finding) in input.findings.iter().enumerate() {
            validate_analysis_text(
                "write_analysis",
                &format!("findings[{index}]"),
                &finding.text,
                &finding.source_blocks,
                &source_blocks,
            )?;
        }
        let artifact = self.service.persist_json_artifact(
            &input.job_id,
            crate::ArtifactKind::Analysis,
            serde_json::json!({
                "schemaVersion": "1.1",
                "articleId": input.article_id,
                "centralArgument": input.central_argument,
                "findings": input.findings,
            }),
        )?;
        Ok(WriteAnalysisOutput {
            artifact_id: artifact.artifact_id,
            content_hash: artifact.content_hash,
        })
    }

    pub fn read_artifact(
        &self,
        input: ReadArtifactInput,
    ) -> Result<ReadArtifactOutput, DigestError> {
        Ok(ReadArtifactOutput {
            artifact: self.service.read_artifact(&input.artifact_id)?,
        })
    }

    pub async fn ingest_article(
        &self,
        input: IngestArticleInput,
    ) -> Result<IngestArticleOutput, IngestionError> {
        let result = ArticleIngestionService::new(self.service.clone())
            .ingest_url(&input.job_id, &input.url)
            .await?;
        Ok(IngestArticleOutput {
            source_artifact_id: result.source.artifact_id,
            article_artifact_id: result.article.artifact_id,
        })
    }

    pub fn write_narration_plan(
        &self,
        input: WriteNarrationPlanInput,
    ) -> Result<ArtifactEnvelope, DigestError> {
        let missing = [
            ("jobId", input.job_id.trim().is_empty()),
            ("articleId", input.article_id.trim().is_empty()),
            ("title", input.title.trim().is_empty()),
            ("segments", input.segments.is_empty()),
        ]
        .into_iter()
        .filter_map(|(field, invalid)| invalid.then_some(field))
        .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(DigestError::InvalidInput(format!(
                "write_narration_plan: narration plan requires jobId, articleId, title, and \
                 segments; missing or empty: {}. `jobId` is the running job, `articleId` is the \
                 normalized article id from ingest_article, `title` is the lesson title, and \
                 `segments` is a non-empty array of narration segment drafts. \
                 Corrected call: {}",
                missing.join(", "),
                crate::tool_input::WRITE_NARRATION_PLAN_EXAMPLE
            )));
        }
        let article = self.service.read_artifact(&input.article_id)?;
        if article.job_id != input.job_id || article.kind != crate::ArtifactKind::NormalizedArticle
        {
            return Err(article_mismatch_error(
                "write_narration_plan",
                &input.article_id,
                &input.job_id,
                &article,
            ));
        }
        let source_blocks = source_block_ids(&article);
        let diagram_blocks: HashSet<&str> = article
            .payload
            .get("blocks")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|block| {
                block.get("kind").and_then(serde_json::Value::as_str) == Some("diagram")
            })
            .filter_map(|block| block.get("id").and_then(serde_json::Value::as_str))
            .collect();
        let referenced_blocks: HashSet<String> = input
            .segments
            .iter()
            .flat_map(|segment| segment.source_blocks.iter().cloned())
            .collect();
        let presented_diagram_blocks: HashSet<String> = input
            .segments
            .iter()
            .filter(|segment| segment.presentation_type == PresentationType::Diagram)
            .flat_map(|segment| segment.source_blocks.iter())
            .filter(|source_block| diagram_blocks.contains(source_block.as_str()))
            .cloned()
            .collect();
        let coverage_counts = validate_source_coverage(
            &input.source_coverage_decisions,
            &source_blocks,
            &diagram_blocks,
            &referenced_blocks,
            &presented_diagram_blocks,
        )?;
        let core_segment_count = input
            .segments
            .iter()
            .filter(|segment| matches!(segment.importance, NarrationImportance::Core))
            .count();
        let source_coverage_percent = if source_blocks.is_empty() {
            0
        } else {
            referenced_blocks.len() * 100 / source_blocks.len()
        };
        let diagram_coverage_percent = if diagram_blocks.is_empty() {
            100
        } else {
            presented_diagram_blocks.len() * 100 / diagram_blocks.len()
        };
        let narration_word_count = input
            .segments
            .iter()
            .map(|segment| word_count(&segment.display_text))
            .sum::<usize>();
        let source_word_count = article
            .payload
            .get("diagnostics")
            .and_then(|diagnostics| diagnostics.get("wordCount"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default() as usize;
        let narration_to_source_word_percent = narration_word_count
            .saturating_mul(100)
            .checked_div(source_word_count)
            .unwrap_or_default();
        let mut segments = Vec::with_capacity(input.segments.len());
        for (index, segment) in input.segments.into_iter().enumerate() {
            let path = format!("segments[{index}]");
            let mut empty = Vec::new();
            if segment.display_text.trim().is_empty() {
                empty.push("displayText (empty or whitespace only)");
            }
            if segment.tts_text.trim().is_empty() {
                empty.push("ttsText (empty or whitespace only)");
            }
            if segment.source_blocks.is_empty() {
                empty.push("sourceBlocks (empty array)");
            }
            if !empty.is_empty() {
                return Err(DigestError::InvalidInput(format!(
                    "write_narration_plan: narration segment {} is incomplete: {}. Every segment \
                     must carry a non-empty `displayText`, a non-empty `ttsText`, and a non-empty \
                     `sourceBlocks` array of article block ids. Corrected segment: {}",
                    index + 1,
                    empty.join("; "),
                    crate::tool_input::NARRATION_SEGMENT_EXAMPLE
                )));
            }
            validate_tts_normalization(index, &segment.display_text, &segment.tts_text)?;
            if let Some(unknown) = segment
                .source_blocks
                .iter()
                .find(|source_block| !source_blocks.contains(source_block.as_str()))
            {
                return Err(unknown_source_block_error(
                    "write_narration_plan",
                    &format!("{path}.sourceBlocks"),
                    unknown,
                    &source_blocks,
                ));
            }
            if segment.presentation_type == PresentationType::Diagram
                && !segment
                    .source_blocks
                    .iter()
                    .any(|source_block| diagram_blocks.contains(source_block.as_str()))
            {
                return Err(DigestError::InvalidInput(format!(
                    "write_narration_plan: narration segment {} sets `presentationType` to \
                     `diagram` but cites no diagram block. The article has {} diagram block(s): {}. \
                     Fix by citing one of them in segments[{index}].sourceBlocks, or by using a \
                     different `presentationType` such as `article-text` or `concept-card`.",
                    index + 1,
                    diagram_blocks.len(),
                    describe_source_blocks(&diagram_blocks)
                )));
            }
            segments.push(serde_json::json!({
                "id": format!("segment-{}", index + 1),
                "displayText": segment.display_text,
                "ttsText": segment.tts_text,
                "sourceBlocks": segment.source_blocks,
                "provenance": {"type": segment.provenance},
                "presentation": {"type": segment.presentation_type},
                "importance": segment.importance,
                "intent": segment.intent,
            }));
        }
        self.service.persist_json_artifact(
            &input.job_id,
            crate::ArtifactKind::NarrationPlan,
            serde_json::json!({
                "schemaVersion": "1.3",
                "articleId": input.article_id,
                "title": input.title,
                "segments": segments,
                "sourceCoverageDecisions": input.source_coverage_decisions,
                "diagnostics": {
                    "referencedBlockCount": referenced_blocks.len(),
                    "sourceBlockCount": source_blocks.len(),
                    "sourceCoveragePercent": source_coverage_percent,
                    "accountedBlockCount": coverage_counts.accounted,
                    "taughtBlockCount": coverage_counts.taught,
                    "summarizedBlockCount": coverage_counts.summarized,
                    "skippedBlockCount": coverage_counts.skipped,
                    "coreSegmentCount": core_segment_count,
                    "diagramBlockCount": diagram_blocks.len(),
                    "referencedDiagramCount": presented_diagram_blocks.len(),
                    "diagramCoveragePercent": diagram_coverage_percent,
                    "narrationWordCount": narration_word_count,
                    "sourceWordCount": source_word_count,
                    "narrationToSourceWordPercent": narration_to_source_word_percent,
                },
            }),
        )
    }
}

#[derive(Default)]
struct CoverageCounts {
    accounted: usize,
    taught: usize,
    summarized: usize,
    skipped: usize,
}

fn validate_source_coverage(
    decisions: &[SourceCoverageDecision],
    source_blocks: &HashSet<&str>,
    diagram_blocks: &HashSet<&str>,
    referenced_blocks: &HashSet<String>,
    presented_diagram_blocks: &HashSet<String>,
) -> Result<CoverageCounts, DigestError> {
    let mut seen = HashSet::new();
    let mut counts = CoverageCounts::default();
    for (index, decision) in decisions.iter().enumerate() {
        let path = format!("sourceCoverageDecisions[{index}]");
        let mut empty = Vec::new();
        if decision.source_blocks.is_empty() {
            empty.push("sourceBlocks (empty array)");
        }
        if decision.rationale.trim().is_empty() {
            empty.push("rationale (empty or whitespace only)");
        }
        if !empty.is_empty() {
            return Err(DigestError::InvalidInput(format!(
                "write_narration_plan: source coverage decision {} is incomplete: {}. Every \
                 decision needs a non-empty `sourceBlocks` array and a non-empty `rationale`. \
                 Corrected decision: {}",
                index + 1,
                empty.join("; "),
                crate::tool_input::COVERAGE_DECISION_EXAMPLE
            )));
        }
        for source_block in &decision.source_blocks {
            if !source_blocks.contains(source_block.as_str()) {
                return Err(unknown_source_block_error(
                    "write_narration_plan",
                    &format!("{path}.sourceBlocks"),
                    source_block,
                    source_blocks,
                ));
            }
            if !seen.insert(source_block.as_str()) {
                return Err(DigestError::InvalidInput(format!(
                    "write_narration_plan: source block {source_block} has more than one coverage \
                     decision. Each block may be covered exactly once, and the block was already \
                     claimed by an earlier decision. Remove the duplicate from \
                     {path}.sourceBlocks, or merge it into the decision that already lists \
                     {source_block}. Corrected decision: {}",
                    crate::tool_input::COVERAGE_DECISION_EXAMPLE
                )));
            }
            let referenced = referenced_blocks.contains(source_block);
            match decision.treatment {
                SourceCoverageTreatment::Teach | SourceCoverageTreatment::Summarize => {
                    if !referenced {
                        return Err(DigestError::InvalidInput(format!(
                            "write_narration_plan: source block {source_block} is marked {:?} in \
                             {path} but is not cited by a narration segment. Blocks treated as \
                             `teach` or `summarize` must appear in at least one segment's \
                             `sourceBlocks`. Fix by citing {source_block} in some segment, or by \
                             changing {path}.treatment to `skip` with a rationale that says why the \
                             block is not narrated.",
                            decision.treatment
                        )));
                    }
                    if diagram_blocks.contains(source_block.as_str())
                        && !presented_diagram_blocks.contains(source_block)
                    {
                        return Err(DigestError::InvalidInput(format!(
                            "write_narration_plan: diagram source block {source_block} is selected \
                             in {path} but is not presented by a diagram segment. A diagram block \
                             that is taught or summarized must be shown by a segment whose \
                             `presentationType` is `diagram`. Fix by adding or retitling a segment \
                             with `presentationType` `diagram` that cites {source_block}, or by \
                             changing {path}.treatment to `skip`."
                        )));
                    }
                }
                SourceCoverageTreatment::Skip if referenced => {
                    return Err(DigestError::InvalidInput(format!(
                        "write_narration_plan: source block {source_block} is marked skip in \
                         {path} but is cited by a narration segment. The rule is: a block marked \
                         `skip` must not appear in any segment's `sourceBlocks`. Fix either by \
                         removing {source_block} from the `sourceBlocks` of every segment that \
                         cites it and keeping treatment `skip`, or by changing {path}.treatment to \
                         `teach` or `summarize` so the existing citation is accounted for."
                    )));
                }
                SourceCoverageTreatment::Skip => {}
            }
            counts.accounted += 1;
            match decision.treatment {
                SourceCoverageTreatment::Teach => counts.taught += 1,
                SourceCoverageTreatment::Summarize => counts.summarized += 1,
                SourceCoverageTreatment::Skip => counts.skipped += 1,
            }
        }
    }
    if seen.len() != source_blocks.len() {
        let mut uncovered = source_blocks
            .iter()
            .filter(|source_block| !seen.contains(**source_block))
            .copied()
            .collect::<Vec<_>>();
        uncovered.sort_by_key(|id| block_id_order(id));
        return Err(DigestError::InvalidInput(format!(
            "write_narration_plan: source coverage decisions account for {} of {} source blocks. \
             Unaccounted block(s): {}. Every block in the article must appear in exactly one \
             decision. Fix by adding a decision for the listed blocks, for example {}",
            seen.len(),
            source_blocks.len(),
            uncovered.join(", "),
            crate::tool_input::COVERAGE_DECISION_EXAMPLE
        )));
    }
    Ok(counts)
}

fn source_block_ids(article: &ArtifactEnvelope) -> HashSet<&str> {
    article
        .payload
        .get("blocks")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| block.get("id").and_then(serde_json::Value::as_str))
        .collect()
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn article_mismatch_error(
    tool: &str,
    article_id: &str,
    job_id: &str,
    article: &ArtifactEnvelope,
) -> DigestError {
    DigestError::InvalidInput(format!(
        "{tool}: articleId \"{article_id}\" does not reference this job's normalized article. It \
         resolved to artifact \"{}\", which is a {:?} owned by job \"{}\", not a NormalizedArticle \
         owned by job \"{job_id}\". Fix by calling ingest_article again and using its \
         `articleArtifactId`, and by passing the same `jobId` you started the run with.",
        article.artifact_id, article.kind, article.job_id
    ))
}

fn unknown_source_block_error(
    tool: &str,
    field_path: &str,
    unknown: &str,
    source_blocks: &HashSet<&str>,
) -> DigestError {
    DigestError::InvalidInput(format!(
        "{tool}: {field_path} references unknown source block \"{unknown}\". The normalized \
         article has {}: {}. Block ids come from the article artifact's `blocks[].id`, so read the \
         article with read_artifact and copy the ids from there. Fix by replacing \"{unknown}\" \
         with one of the ids listed above, or by citing no block if the segment has no source.",
        source_blocks.len(),
        describe_source_blocks(source_blocks)
    ))
}

/// Quotes a bounded, id-ordered sample of block ids so the agent can copy a valid one.
fn describe_source_blocks(source_blocks: &HashSet<&str>) -> String {
    let mut ids = source_blocks.iter().copied().collect::<Vec<_>>();
    ids.sort_by_key(|id| block_id_order(id));
    if ids.is_empty() {
        return "(none)".to_owned();
    }
    if ids.len() <= BLOCK_SAMPLE {
        return ids.join(", ");
    }
    format!(
        "{} (first {BLOCK_SAMPLE} of {})",
        ids[..BLOCK_SAMPLE].join(", "),
        ids.len()
    )
}

fn block_id_order(id: &str) -> (u64, &str) {
    match id
        .rsplit_once('-')
        .and_then(|(_, number)| number.parse::<u64>().ok())
    {
        Some(number) => (number, ""),
        None => (u64::MAX, id),
    }
}

fn validate_analysis_claim(
    tool: &str,
    field_path: &str,
    claim: &AnalysisClaim,
    source_blocks: &HashSet<&str>,
) -> Result<(), DigestError> {
    validate_analysis_text(
        tool,
        field_path,
        &claim.text,
        &claim.source_blocks,
        source_blocks,
    )
}

fn validate_analysis_text(
    tool: &str,
    field_path: &str,
    text: &str,
    claim_source_blocks: &[String],
    source_blocks: &HashSet<&str>,
) -> Result<(), DigestError> {
    let mut empty = Vec::new();
    if text.trim().is_empty() {
        empty.push("text (empty or whitespace only)");
    }
    if claim_source_blocks.is_empty() {
        empty.push("sourceBlocks (empty array)");
    }
    if !empty.is_empty() {
        return Err(DigestError::InvalidInput(format!(
            "{tool}: {field_path} is incomplete: {}. Every claim needs a non-empty `text` and a \
             non-empty `sourceBlocks` array of article block ids. Corrected value: {}",
            empty.join("; "),
            crate::tool_input::ANALYSIS_CLAIM_EXAMPLE
        )));
    }
    if let Some(unknown) = claim_source_blocks
        .iter()
        .find(|source_block| !source_blocks.contains(source_block.as_str()))
    {
        return Err(unknown_source_block_error(
            tool,
            &format!("{field_path}.sourceBlocks"),
            unknown,
            source_blocks,
        ));
    }
    Ok(())
}

fn validate_tts_normalization(
    index: usize,
    display_text: &str,
    tts_text: &str,
) -> Result<(), DigestError> {
    let display_tokens = spoken_tokens(display_text);
    let tts_tokens = spoken_tokens(tts_text);
    let allowed_edits = allowed_token_edits(display_tokens.len());
    if token_edit_distance_exceeds(&display_tokens, &tts_tokens, allowed_edits) {
        return Err(DigestError::InvalidInput(format!(
            "write_narration_plan: narration segment {} ttsText may only normalize pronunciation, \
             not add or remove content.\n\
             Measured: {} spoken token(s) in displayText against {} in ttsText, token edit \
             distance {}, allowed budget {}.\n\
             Accepted normalization: {ACCEPTED_NORMALIZATION}.\n\
             Rejected rewrite: {REJECTED_REWRITE}.\n\
             Fix: make ttsText word-for-word identical to displayText, changing only how a term is \
             pronounced (spelling out an abbreviation, expanding a symbol, respelling an \
             identifier). If you want to add explanation, put it in displayText instead.",
            index + 1,
            display_tokens.len(),
            tts_tokens.len(),
            describe_token_edit_distance(&display_tokens, &tts_tokens),
            allowed_edits
        )));
    }
    Ok(())
}

fn allowed_token_edits(display_token_count: usize) -> usize {
    5.max(display_token_count.div_ceil(10))
}

fn describe_token_edit_distance(display_tokens: &[String], tts_tokens: &[String]) -> String {
    match token_edit_distance(display_tokens, tts_tokens) {
        Some(distance) => distance.to_string(),
        None => format!(
            "not computed, because {} by {} exceeds the {MAX_DISTANCE_CELLS}-cell reporting guard; \
             the lengths above already exceed the budget",
            display_tokens.len(),
            tts_tokens.len()
        ),
    }
}

fn spoken_tokens(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Exact Levenshtein distance over spoken tokens, used only to report a measured number after
/// `token_edit_distance_exceeds` has already rejected the segment. The accept/reject decision
/// never depends on it.
fn token_edit_distance(left: &[String], right: &[String]) -> Option<usize> {
    if left.len().saturating_mul(right.len()) > MAX_DISTANCE_CELLS {
        return None;
    }
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_token) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_token) in right.iter().enumerate() {
            current[right_index + 1] = (previous[right_index]
                + usize::from(left_token != right_token))
            .min(previous[right_index + 1] + 1)
            .min(current[right_index] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    Some(previous[right.len()])
}

fn token_edit_distance_exceeds(left: &[String], right: &[String], limit: usize) -> bool {
    if left.len().abs_diff(right.len()) > limit {
        return true;
    }
    if left.is_empty() || right.is_empty() {
        return left.len().max(right.len()) > limit;
    }
    let sentinel = limit + 1;
    let mut previous = vec![sentinel; right.len() + 1];
    for (index, value) in previous
        .iter_mut()
        .take(limit.min(right.len()) + 1)
        .enumerate()
    {
        *value = index;
    }
    let mut current = vec![sentinel; right.len() + 1];
    for (left_index, left_token) in left.iter().enumerate() {
        current.fill(sentinel);
        let row = left_index + 1;
        if row <= limit {
            current[0] = row;
        }
        let start = row.saturating_sub(limit).max(1);
        let end = (row + limit).min(right.len());
        for column in start..=end {
            current[column] = if left_token == &right[column - 1] {
                previous[column - 1]
            } else {
                1 + previous[column - 1]
                    .min(previous[column])
                    .min(current[column - 1])
            };
        }
        if current[start..=end]
            .iter()
            .all(|distance| *distance > limit)
        {
            return true;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()] > limit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_input::Vocabulary;

    fn tokens(text: &str) -> Vec<String> {
        spoken_tokens(text)
    }

    /// The names a message will list, derived from the variants themselves.
    fn vocabulary_names<T: Copy + serde::Serialize>(variants: &[T]) -> String {
        Vocabulary::new("test", variants, "").names
    }

    #[test]
    fn banded_and_exact_edit_distances_agree_on_the_acceptance_boundary() {
        let display = tokens("io_uring submits work asynchronously.");
        let tts = tokens("eye-oh uring submits work asynchronously.");
        let allowed = allowed_token_edits(display.len());
        assert_eq!(allowed, 5);
        assert!(!token_edit_distance_exceeds(&display, &tts, allowed));
        assert_eq!(token_edit_distance(&display, &tts), Some(2));

        let rewritten = tokens(
            "The WAL records every acknowledged write. This architecture also makes every replica \
             independently scalable.",
        );
        let reference = tokens("The WAL records every acknowledged write.");
        let allowed = allowed_token_edits(reference.len());
        assert!(token_edit_distance_exceeds(&reference, &rewritten, allowed));
        let measured = token_edit_distance(&reference, &rewritten).expect("distance is computable");
        assert!(measured > allowed);
    }

    #[test]
    fn every_enum_exposes_a_serializable_vocabulary_matching_its_documented_names() {
        assert_eq!(
            vocabulary_names(&ProvenanceKind::ALL),
            "source_derived, ai_explanation, ai_inference, generated_educational"
        );
        assert_eq!(
            vocabulary_names(&AnalysisFindingKind::ALL),
            "major_concept, supporting_concept, key_claim, example, definition, comparison, \
             causal_relationship, code_example, quantitative_claim, important_entity, \
             difficult_section, prerequisite, visualization_opportunity, point_of_confusion"
        );
        assert_eq!(
            vocabulary_names(&PresentationType::ALL),
            "article-text, callout, code, concept-card, diagram, image, quote"
        );
        assert_eq!(vocabulary_names(&NarrationImportance::ALL), "core, supporting");
        assert_eq!(
            vocabulary_names(&NarrationIntent::ALL),
            "introduction, explanation, example, comparison, quantification, takeaway"
        );
        assert_eq!(
            vocabulary_names(&SourceCoverageTreatment::ALL),
            "teach, summarize, skip"
        );
    }

    #[test]
    fn block_samples_are_id_ordered_and_bounded() {
        let blocks = ["block-10", "block-2", "block-1"]
            .into_iter()
            .collect::<HashSet<_>>();
        assert_eq!(
            describe_source_blocks(&blocks),
            "block-1, block-2, block-10"
        );

        let many = (1..=12)
            .map(|index| format!("block-{index}"))
            .collect::<Vec<_>>();
        let many = many.iter().map(String::as_str).collect::<HashSet<_>>();
        let described = describe_source_blocks(&many);
        assert!(
            described.starts_with(
                "block-1, block-2, block-3, block-4, block-5, block-6, \
             block-7, block-8 (first 8 of 12)"
            ),
            "{described}"
        );
    }
}
