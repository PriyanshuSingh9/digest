use crate::{
    DigestTools, ReadArtifactInput, ReadArtifactOutput, WriteAnalysisInput, WriteAnalysisOutput,
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
}

#[tool_router(server_handler)]
impl DigestMcpServer {
    #[tool(
        name = "write_analysis",
        description = "Validate and persist a structured article analysis in the active Digest job"
    )]
    fn write_analysis(
        &self,
        Parameters(input): Parameters<WriteAnalysisInput>,
    ) -> Result<Json<WriteAnalysisOutput>, ErrorData> {
        self.tools
            .write_analysis(input)
            .map(Json)
            .map_err(tool_error)
    }

    #[tool(
        name = "read_artifact",
        description = "Read one persisted Digest artifact by its immutable artifact identifier"
    )]
    fn read_artifact(
        &self,
        Parameters(input): Parameters<ReadArtifactInput>,
    ) -> Result<Json<ReadArtifactOutput>, ErrorData> {
        self.tools
            .read_artifact(input)
            .map(Json)
            .map_err(tool_error)
    }
}

fn tool_error(error: crate::DigestError) -> ErrorData {
    ErrorData::internal_error(error.to_string(), None)
}
