use crate::{
    AcpClient, AgentEvent, AgentProvider, AgentRunCancellation, AgentRunRequest, AgentRunResult,
    AgentRunSupervision, ArticleIngestionService, ArtifactEnvelope, ArtifactKind, DigestService,
    McpLaunchSpec, PermissionPolicy, RunAttempt, RunSummary,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Manager, State};

pub struct HostState {
    service: Arc<DigestService>,
    data_dir: PathBuf,
    executable: PathBuf,
    supervision: AgentRunSupervision,
    active_runs: Mutex<HashMap<String, AgentRunCancellation>>,
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
    pub agent_inactivity_timeout_seconds: u64,
    pub agent_max_runtime_seconds: Option<u64>,
}

struct ActiveRunGuard<'a> {
    job_id: String,
    active_runs: &'a Mutex<HashMap<String, AgentRunCancellation>>,
}

impl Drop for ActiveRunGuard<'_> {
    fn drop(&mut self) {
        self.active_runs
            .lock()
            .expect("active runs lock poisoned")
            .remove(&self.job_id);
    }
}

#[tauri::command]
pub fn host_info(state: State<'_, HostState>) -> HostInfo {
    HostInfo {
        data_dir: state.data_dir.clone(),
        mcp_executable: state.executable.clone(),
        agent_inactivity_timeout_seconds: state.supervision.inactivity_timeout.as_secs(),
        agent_max_runtime_seconds: state
            .supervision
            .max_runtime
            .map(|duration| duration.as_secs()),
    }
}

#[tauri::command]
pub fn run_snapshot(
    state: State<'_, HostState>,
    job_id: String,
    after_event_sequence: Option<i64>,
) -> Result<RunSnapshot, String> {
    Ok(RunSnapshot {
        attempts: state
            .service
            .list_run_attempts(&job_id)
            .map_err(|error| error.to_string())?,
        events: match after_event_sequence {
            Some(sequence) => state
                .service
                .list_presentation_events_after(&job_id, sequence),
            None => state.service.list_presentation_events(&job_id),
        }
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
    let cancellation = AgentRunCancellation::new();
    {
        let mut active_runs = state.active_runs.lock().expect("active runs lock poisoned");
        if active_runs.contains_key(&input.job_id) {
            return Err(format!(
                "an agent run is already active for job {}",
                input.job_id
            ));
        }
        active_runs.insert(input.job_id.clone(), cancellation.clone());
    }
    let _active_run = ActiveRunGuard {
        job_id: input.job_id.clone(),
        active_runs: &state.active_runs,
    };
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
        .with_supervision(state.supervision)
        .run_once_with_cancellation(
            AgentRunRequest {
                prompt: format!(
                    "The active Digest job ID is `{}`. Digest already captured and normalized the \
                     article. Read normalized article artifact `{}` with Digest MCP instead of \
                     fetching the URL yourself. Use that artifact ID as the article ID when writing \
                     analysis. In write_analysis, categorize the article's important concepts, claims, \
                     examples, difficult sections, and visualization opportunities as findings. Cite \
                     source block IDs for the central argument and every finding, and mark AI inference \
                     or explanation explicitly. After analysis, call \
                     write_narration_plan with source-grounded segments. Do not optimize for a short \
                     summary: preserve the depth needed to teach the article. Label each segment's \
                     learning intent, importance, and provenance. Account for every normalized source \
                     block exactly once in sourceCoverageDecisions by choosing teach, summarize, or \
                     skip with a rationale; decisions may group related blocks. Every taught or \
                     summarized block must be cited by a narration segment. For every diagram block, \
                     either present it in a diagram segment or explicitly skip it with a rationale. \
                     Keep displayText faithful to the source. Use ttsText only for pronunciation \
                     normalization; preserve established acronyms and technical names unless their \
                     spoken form is known to need changing.\n\n{}",
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
            },
            cancellation,
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn cancel_agent_run(state: State<'_, HostState>, job_id: String) -> Result<(), String> {
    let active_runs = state.active_runs.lock().expect("active runs lock poisoned");
    let cancellation = active_runs
        .get(&job_id)
        .ok_or_else(|| format!("no agent run is active for job {job_id}"))?;
    cancellation.cancel();
    Ok(())
}

pub fn configure_host(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let executable = std::env::current_exe()?;
    let supervision = supervision_from_environment()?;
    let service = Arc::new(DigestService::open(&data_dir)?);
    service.recover_abandoned_attempts()?;
    app.manage(HostState {
        service,
        data_dir,
        executable,
        supervision,
        active_runs: Mutex::new(HashMap::new()),
    });
    Ok(())
}

fn supervision_from_environment() -> Result<AgentRunSupervision, Box<dyn std::error::Error>> {
    let defaults = AgentRunSupervision::default();
    Ok(AgentRunSupervision {
        inactivity_timeout: duration_from_environment(
            "DIGEST_AGENT_INACTIVITY_TIMEOUT_SECS",
            Some(defaults.inactivity_timeout),
        )?
        .expect("inactivity timeout has a default"),
        max_runtime: duration_from_environment("DIGEST_AGENT_MAX_RUNTIME_SECS", None)?,
    })
}

fn duration_from_environment(
    name: &str,
    default: Option<Duration>,
) -> Result<Option<Duration>, Box<dyn std::error::Error>> {
    let Some(value) = std::env::var_os(name) else {
        return Ok(default);
    };
    let seconds = value.to_string_lossy().parse::<u64>()?;
    if seconds == 0 {
        return Err(format!("{name} must be greater than zero").into());
    }
    Ok(Some(Duration::from_secs(seconds)))
}
