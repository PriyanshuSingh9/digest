use crate::{
    ArticleIngestionService, ArtifactEnvelope, DigestError, DigestService, IngestionError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    SourceDerived,
    Inference,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisClaim {
    pub text: String,
    pub source_blocks: Vec<String>,
    pub kind: ClaimKind,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteAnalysisInput {
    pub job_id: String,
    pub article_id: String,
    pub central_argument: AnalysisClaim,
    pub learning_points: Vec<AnalysisClaim>,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteAnalysisOutput {
    pub artifact_id: String,
    pub content_hash: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadArtifactInput {
    pub artifact_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadArtifactOutput {
    pub artifact: ArtifactEnvelope,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngestArticleInput {
    pub job_id: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngestArticleOutput {
    pub source_artifact_id: String,
    pub article_artifact_id: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NarrationSegmentDraft {
    pub display_text: String,
    pub tts_text: String,
    pub source_blocks: Vec<String>,
    pub presentation_type: PresentationType,
    pub importance: NarrationImportance,
    pub intent: NarrationIntent,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresentationType {
    ArticleText,
    Callout,
    Code,
    ConceptCard,
    Diagram,
    Image,
    Quote,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrationImportance {
    Core,
    Supporting,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrationIntent {
    Introduction,
    Explanation,
    Example,
    Comparison,
    Takeaway,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteNarrationPlanInput {
    pub job_id: String,
    pub article_id: String,
    pub title: String,
    pub segments: Vec<NarrationSegmentDraft>,
}

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
        if input.job_id.trim().is_empty()
            || input.article_id.trim().is_empty()
            || input.learning_points.is_empty()
        {
            return Err(DigestError::InvalidInput(
                "analysis requires jobId, articleId, centralArgument, and learningPoints".into(),
            ));
        }
        let article = self.service.read_artifact(&input.article_id)?;
        if article.job_id != input.job_id || article.kind != crate::ArtifactKind::NormalizedArticle
        {
            return Err(DigestError::InvalidInput(
                "analysis articleId must reference this job's normalized article".into(),
            ));
        }
        let source_blocks: HashSet<&str> = article
            .payload
            .get("blocks")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|block| block.get("id").and_then(serde_json::Value::as_str))
            .collect();
        validate_analysis_claim("centralArgument", &input.central_argument, &source_blocks)?;
        for (index, claim) in input.learning_points.iter().enumerate() {
            validate_analysis_claim(&format!("learningPoints[{}]", index), claim, &source_blocks)?;
        }
        let artifact = self.service.persist_json_artifact(
            &input.job_id,
            crate::ArtifactKind::Analysis,
            serde_json::json!({
                "schemaVersion": "1.1",
                "articleId": input.article_id,
                "centralArgument": input.central_argument,
                "learningPoints": input.learning_points,
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
        if input.job_id.trim().is_empty()
            || input.article_id.trim().is_empty()
            || input.title.trim().is_empty()
            || input.segments.is_empty()
        {
            return Err(DigestError::InvalidInput(
                "narration plan requires jobId, articleId, title, and segments".into(),
            ));
        }
        let article = self.service.read_artifact(&input.article_id)?;
        if article.job_id != input.job_id || article.kind != crate::ArtifactKind::NormalizedArticle
        {
            return Err(DigestError::InvalidInput(
                "narration plan articleId must reference this job's normalized article".into(),
            ));
        }
        let source_blocks: HashSet<&str> = article
            .payload
            .get("blocks")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|block| block.get("id").and_then(serde_json::Value::as_str))
            .collect();
        let referenced_blocks: HashSet<String> = input
            .segments
            .iter()
            .flat_map(|segment| segment.source_blocks.iter().cloned())
            .collect();
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
        let mut segments = Vec::with_capacity(input.segments.len());
        for (index, segment) in input.segments.into_iter().enumerate() {
            if segment.display_text.trim().is_empty()
                || segment.tts_text.trim().is_empty()
                || segment.source_blocks.is_empty()
            {
                return Err(DigestError::InvalidInput(format!(
                    "narration segment {} is incomplete",
                    index + 1
                )));
            }
            if let Some(unknown) = segment
                .source_blocks
                .iter()
                .find(|source_block| !source_blocks.contains(source_block.as_str()))
            {
                return Err(DigestError::InvalidInput(format!(
                    "narration segment {} references unknown source block {unknown}",
                    index + 1
                )));
            }
            segments.push(serde_json::json!({
                "id": format!("segment-{}", index + 1),
                "displayText": segment.display_text,
                "ttsText": segment.tts_text,
                "sourceBlocks": segment.source_blocks,
                "provenance": {"type": "source_derived"},
                "presentation": {"type": segment.presentation_type},
                "importance": segment.importance,
                "intent": segment.intent,
            }));
        }
        self.service.persist_json_artifact(
            &input.job_id,
            crate::ArtifactKind::NarrationPlan,
            serde_json::json!({
                "schemaVersion": "1.1",
                "articleId": input.article_id,
                "title": input.title,
                "segments": segments,
                "diagnostics": {
                    "referencedBlockCount": referenced_blocks.len(),
                    "sourceBlockCount": source_blocks.len(),
                    "sourceCoveragePercent": source_coverage_percent,
                    "coreSegmentCount": core_segment_count,
                },
            }),
        )
    }
}

fn validate_analysis_claim(
    field: &str,
    claim: &AnalysisClaim,
    source_blocks: &HashSet<&str>,
) -> Result<(), DigestError> {
    if claim.text.trim().is_empty() || claim.source_blocks.is_empty() {
        return Err(DigestError::InvalidInput(format!(
            "{field} requires text and sourceBlocks"
        )));
    }
    if let Some(unknown) = claim
        .source_blocks
        .iter()
        .find(|source_block| !source_blocks.contains(source_block.as_str()))
    {
        return Err(DigestError::InvalidInput(format!(
            "{field} references unknown source block {unknown}"
        )));
    }
    Ok(())
}
