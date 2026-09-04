use crate::{
    AcpClient, AgentEvent, AgentProvider, AgentRunRequest, AgentRunResult, ArtifactEnvelope,
    DigestService, McpLaunchSpec, PermissionPolicy,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tauri::{Manager, State};

pub struct HostState {
    service: Arc<DigestService>,
    data_dir: PathBuf,
    executable: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAgentRun {
    pub job_id: String,
    pub provider: AgentProvider,
    pub cwd: PathBuf,
    pub prompt: String,
    #[serde(default)]
    pub allow_once_permissions: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSnapshot {
    pub events: Vec<AgentEvent>,
    pub artifacts: Vec<ArtifactEnvelope>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub data_dir: PathBuf,
    pub mcp_executable: PathBuf,
}

#[tauri::command]
pub fn host_info(state: State<'_, HostState>) -> HostInfo {
    HostInfo {
        data_dir: state.data_dir.clone(),
        mcp_executable: state.executable.clone(),
    }
}

#[tauri::command]
pub fn run_snapshot(state: State<'_, HostState>, job_id: String) -> Result<RunSnapshot, String> {
    Ok(RunSnapshot {
        events: state
            .service
            .list_agent_events(&job_id)
            .map_err(|error| error.to_string())?,
        artifacts: state
            .service
            .list_artifacts(&job_id)
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn start_agent_run(
    state: State<'_, HostState>,
    input: StartAgentRun,
) -> Result<AgentRunResult, String> {
    let digest_mcp = McpLaunchSpec::new(state.executable.clone(), state.data_dir.clone())
        .map_err(|error| error.to_string())?
        .with_prefix_args(vec!["--digest-mcp".into()]);
    AcpClient::new(Arc::clone(&state.service))
        .run_once(AgentRunRequest {
            prompt: format!(
                "The active Digest job ID is `{}` and the article ID for this evaluation is \
                 `phase-0-evaluation`. Pass those exact identifiers to Digest MCP tools.\n\n{}",
                input.job_id, input.prompt
            ),
            job_id: input.job_id,
            provider: input.provider,
            cwd: input.cwd,
            digest_mcp,
            permission_policy: if input.allow_once_permissions {
                PermissionPolicy::AllowOnce
            } else {
                PermissionPolicy::Deny
            },
        })
        .await
        .map_err(|error| error.to_string())
}

pub fn configure_host(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let executable = std::env::current_exe()?;
    let service = Arc::new(DigestService::open(&data_dir)?);
    app.manage(HostState {
        service,
        data_dir,
        executable,
    });
    Ok(())
}
