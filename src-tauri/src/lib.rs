mod acp;
mod application;
mod host;
mod ingestion;
mod mcp;
mod tools;

pub use acp::{
    AcpClient, AcpClientError, AgentLaunchSpec, AgentProvider, AgentRunRequest, AgentRunResult,
    McpLaunchSpec, PermissionPolicy,
};
pub use application::{
    AgentEvent, AnalysisDraft, ArtifactEnvelope, ArtifactKind, AttemptStatus, DigestError,
    DigestService, NewAgentEvent, NewAgentEventKind, RunAttempt, RunStatus, RunSummary,
    StartRunAttempt,
};
pub use ingestion::{
    ArticleBlock, ArticleImage, ArticleIngestionService, ExtractionDiagnostics, ImageCaptureStatus,
    IngestionError, IngestionResult, NormalizedArticle,
};
pub use mcp::DigestMcpServer;
pub use tools::{
    AnalysisClaim, AnalysisFinding, AnalysisFindingKind, DigestTools, IngestArticleInput,
    IngestArticleOutput, NarrationImportance, NarrationIntent, NarrationSegmentDraft,
    PresentationType, ProvenanceKind, ReadArtifactInput, ReadArtifactOutput, WriteAnalysisInput,
    WriteAnalysisOutput, WriteNarrationPlanInput,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(host::configure_host)
        .invoke_handler(tauri::generate_handler![
            host::host_info,
            host::recent_runs,
            host::run_snapshot,
            host::start_agent_run
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
