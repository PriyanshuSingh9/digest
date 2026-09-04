mod acp;
mod application;
mod host;
mod mcp;
mod tools;

pub use acp::{
    AcpClient, AcpClientError, AgentLaunchSpec, AgentProvider, AgentRunRequest, AgentRunResult,
    McpLaunchSpec, PermissionPolicy,
};
pub use application::{
    AgentEvent, AnalysisDraft, ArtifactEnvelope, ArtifactKind, DigestError, DigestService,
    NewAgentEvent, NewAgentEventKind,
};
pub use mcp::DigestMcpServer;
pub use tools::{
    DigestTools, ReadArtifactInput, ReadArtifactOutput, WriteAnalysisInput, WriteAnalysisOutput,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(host::configure_host)
        .invoke_handler(tauri::generate_handler![
            host::host_info,
            host::run_snapshot,
            host::start_agent_run
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
