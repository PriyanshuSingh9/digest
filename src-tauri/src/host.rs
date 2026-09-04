use crate::{
    AcpClient, AgentEvent, AgentProvider, AgentRunRequest, AgentRunResult, ArticleIngestionService,
    ArtifactEnvelope, ArtifactKind, DigestService, McpLaunchSpec, PermissionPolicy, RunAttempt,
    RunSummary,
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
    pub article_url: String,
    pub prompt: String,
    #[serde(default)]
    pub allow_once_permissions: bool,
    #[serde(default)]
    pub refresh_source: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSnapshot {
    pub attempts: Vec<RunAttempt>,
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
        attempts: state
            .service
            .list_run_attempts(&job_id)
            .map_err(|error| error.to_string())?,
        events: state
            .service
            .list_presentation_events(&job_id)
            .map_err(|error| error.to_string())?,
        artifacts: state
            .service
            .list_artifacts(&job_id)
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub fn recent_runs(
    state: State<'_, HostState>,
    limit: Option<usize>,
) -> Result<Vec<RunSummary>, String> {
    state
        .service
        .list_runs(limit.unwrap_or(20))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn start_agent_run(
    state: State<'_, HostState>,
    input: StartAgentRun,
) -> Result<AgentRunResult, String> {
    let article = if input.refresh_source {
        None
    } else {
        state
            .service
            .latest_artifact(&input.job_id, ArtifactKind::NormalizedArticle)
            .map_err(|error| error.to_string())?
    };
    let article = match article {
        Some(article) => article,
        None => {
            ArticleIngestionService::new(Arc::clone(&state.service))
                .ingest_url(&input.job_id, &input.article_url)
                .await
                .map_err(|error| error.to_string())?
                .article
        }
    };
    let digest_mcp = McpLaunchSpec::new(state.executable.clone(), state.data_dir.clone())
        .map_err(|error| error.to_string())?
        .with_prefix_args(vec!["--digest-mcp".into()]);
    AcpClient::new(Arc::clone(&state.service))
        .run_once(AgentRunRequest {
            prompt: format!(
                "The active Digest job ID is `{}`. Digest already captured and normalized the \
                 article. Read normalized article artifact `{}` with Digest MCP instead of \
                 fetching the URL yourself. Use that artifact ID as the article ID when writing \
                 analysis. In write_analysis, cite source block IDs for the central argument and \
                 every learning point, and mark any inference explicitly. After analysis, call \
                 write_narration_plan exactly once with concise, source-grounded segments. Label \
                 each segment's learning intent and importance. Keep displayText faithful to the \
                 source. Use ttsText only for pronunciation normalization; preserve established \
                 acronyms and technical names unless their spoken form is known to need changing.\n\n{}",
                input.job_id, article.artifact_id, input.prompt
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
    service.recover_abandoned_attempts()?;
    app.manage(HostState {
        service,
        data_dir,
        executable,
    });
    Ok(())
}
