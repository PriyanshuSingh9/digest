use crate::{
    tool_input, ArtifactEnvelope, DigestError, DigestTools, IngestArticleInput,
    IngestArticleOutput, ReadArtifactInput, ReadArtifactOutput, WriteAnalysisInput,
    WriteAnalysisOutput, WriteNarrationPlanInput,
};
use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_router, ErrorData,
};

#[derive(Clone)]
pub struct DigestMcpServer {
    tools: DigestTools,
}

impl DigestMcpServer {
    pub fn new(tools: DigestTools) -> Self {
        Self { tools }
    }

    /// Argument-level entry point for `write_analysis`, shared by the tool handler and tests.
    pub fn write_analysis_arguments(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<WriteAnalysisOutput, ErrorData> {
        let input = tool_input::decode_write_analysis(arguments).map_err(tool_error)?;
        self.tools.write_analysis(input).map_err(tool_error)
    }

    /// Argument-level entry point for `read_artifact`, shared by the tool handler and tests.
    pub fn read_artifact_arguments(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<ReadArtifactOutput, ErrorData> {
        let input = tool_input::decode_read_artifact(arguments).map_err(tool_error)?;
        self.tools.read_artifact(input).map_err(tool_error)
    }

    /// Argument-level entry point for `ingest_article`, shared by the tool handler and tests.
    pub async fn ingest_article_arguments(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<IngestArticleOutput, ErrorData> {
        let input = tool_input::decode_ingest_article(arguments).map_err(tool_error)?;
        self.tools
            .ingest_article(input)
            .await
            .map_err(|error| ingestion_error(error.to_string()))
    }

    /// Argument-level entry point for `write_narration_plan`, shared by the tool handler and tests.
    pub fn write_narration_plan_arguments(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<ArtifactEnvelope, ErrorData> {
        let input = tool_input::decode_write_narration_plan(arguments).map_err(tool_error)?;
        self.tools.write_narration_plan(input).map_err(tool_error)
    }
}

// The handlers take the raw arguments as `serde_json::Value` so the decode above can name the exact
// JSON path, but the published input schema is still the documented input struct's schema.
#[tool_router(server_handler)]
impl DigestMcpServer {
    #[tool(
        name = "write_analysis",
        description = "Validate and persist a structured article analysis in the active Digest job",
        input_schema = rmcp::handler::server::common::schema_for_input::<WriteAnalysisInput>()
            .expect("write_analysis input schema must be a JSON object")
    )]
    fn write_analysis(
        &self,
        Parameters(arguments): Parameters<serde_json::Value>,
    ) -> Result<Json<WriteAnalysisOutput>, ErrorData> {
        self.write_analysis_arguments(&arguments).map(Json)
    }

    #[tool(
        name = "read_artifact",
        description = "Read one persisted Digest artifact by its immutable artifact identifier",
        input_schema = rmcp::handler::server::common::schema_for_input::<ReadArtifactInput>()
            .expect("read_artifact input schema must be a JSON object")
    )]
    fn read_artifact(
        &self,
        Parameters(arguments): Parameters<serde_json::Value>,
    ) -> Result<Json<ReadArtifactOutput>, ErrorData> {
        self.read_artifact_arguments(&arguments).map(Json)
    }

    #[tool(
        name = "ingest_article",
        description = "Safely capture an article URL and persist immutable source and normalized article artifacts",
        input_schema = rmcp::handler::server::common::schema_for_input::<IngestArticleInput>()
            .expect("ingest_article input schema must be a JSON object")
    )]
    async fn ingest_article(
        &self,
        Parameters(arguments): Parameters<serde_json::Value>,
    ) -> Result<Json<IngestArticleOutput>, ErrorData> {
        self.ingest_article_arguments(&arguments).await.map(Json)
    }

    #[tool(
        name = "write_narration_plan",
        description = "Validate and persist a source-grounded narration and presentation plan",
        input_schema = rmcp::handler::server::common::schema_for_input::<WriteNarrationPlanInput>()
            .expect("write_narration_plan input schema must be a JSON object")
    )]
    fn write_narration_plan(
        &self,
        Parameters(arguments): Parameters<serde_json::Value>,
    ) -> Result<Json<ArtifactEnvelope>, ErrorData> {
        self.write_narration_plan_arguments(&arguments).map(Json)
    }
}

/// A bad tool call is the caller's mistake, not a server fault, so it is reported as
/// invalid_params (-32602) rather than internal_error (-32603). Only genuine internal faults keep
/// the internal_error code.
fn tool_error(error: DigestError) -> ErrorData {
    match error {
        DigestError::InvalidInput(message) => ErrorData::invalid_params(message, None),
        DigestError::ArtifactNotFound(message) => ErrorData::invalid_params(
            format!(
                "invalid input: {message}. Fix by reading the id back from the artifact returned \
                 by ingest_article or by the write tools; artifact ids are immutable strings such \
                 as \"article-1\"."
            ),
            None,
        ),
        internal => ErrorData::internal_error(internal.to_string(), None),
    }
}

fn ingestion_error(message: String) -> ErrorData {
    let caller_fixable = message.starts_with("invalid article URL")
        || message.contains("must resolve to a public HTTP address")
        || message.contains("exceeded the");
    if caller_fixable {
        ErrorData::invalid_params(
            format!(
                "invalid input: ingest_article: {message}. Fix the arguments and call again: pass \
                 an absolute http or https url that resolves to a public address and stays inside \
                 the size limits."
            ),
            None,
        )
    } else {
        ErrorData::internal_error(message, None)
    }
}
