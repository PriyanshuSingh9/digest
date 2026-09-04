use crate::{AnalysisDraft, ArtifactEnvelope, DigestError, DigestService};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteAnalysisInput {
    pub job_id: String,
    pub article_id: String,
    pub summary: String,
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
        let artifact = self.service.write_analysis(AnalysisDraft {
            job_id: input.job_id,
            article_id: input.article_id,
            summary: input.summary,
        })?;
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
}
