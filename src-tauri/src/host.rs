use crate::{
    AcpClient, AgentEvent, AgentProvider, AgentRunCancellation, AgentRunRequest, AgentRunResult,
    AgentRunSupervision, ArticleIngestionService, ArtifactEnvelope, ArtifactKind,
    AudioCancellation, AudioGenerationProgress, AudioGenerationRequest, AudioGenerationService,
    AudioProvider, DigestService, GenerateAudioResult, KokoroProvider, McpLaunchSpec,
    PermissionPolicy, RunAttempt, RunSummary,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
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
    active_audio_generations: Mutex<HashMap<String, AudioCancellation>>,
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
    pub kokoro_endpoint: String,
    pub audio_provider: String,
    pub audio_default_voice: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateAudio {
    pub job_id: String,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default = "default_audio_speed")]
    pub speed: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegenerateSegmentAudio {
    pub job_id: String,
    pub segment_id: String,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default = "default_audio_speed")]
    pub speed: f32,
}

struct ActiveJobGuard<'a, T> {
    job_id: String,
    active: &'a Mutex<HashMap<String, T>>,
}

impl<'a, T> ActiveJobGuard<'a, T> {
    /// Registers `entry` for `job_id`, refusing a second concurrent entry,
    /// and removes it again when the guard drops.
    fn register(
        active: &'a Mutex<HashMap<String, T>>,
        job_id: &str,
        entry: T,
        activity: &str,
    ) -> Result<Self, String> {
        let mut entries = active.lock().expect("active job lock poisoned");
        if entries.contains_key(job_id) {
            return Err(format!("{activity} is already active for job {job_id}"));
        }
        entries.insert(job_id.into(), entry);
        Ok(Self {
            job_id: job_id.into(),
            active,
        })
    }
}

impl<T> Drop for ActiveJobGuard<'_, T> {
    fn drop(&mut self) {
        self.active
            .lock()
            .expect("active job lock poisoned")
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
        kokoro_endpoint: kokoro_endpoint(),
        audio_provider: audio_provider().name().into(),
        audio_default_voice: audio_provider().default_voice().into(),
    }
}

#[tauri::command]
pub async fn generate_audio(
    state: State<'_, HostState>,
    input: GenerateAudio,
    on_progress: tauri::ipc::Channel<AudioGenerationProgress>,
) -> Result<GenerateAudioResult, String> {
    run_audio_generation(
        &state,
        &input.job_id,
        input.voice,
        input.speed,
        HashSet::new(),
        on_progress,
    )
    .await
}

#[tauri::command]
pub async fn regenerate_segment_audio(
    state: State<'_, HostState>,
    input: RegenerateSegmentAudio,
    on_progress: tauri::ipc::Channel<AudioGenerationProgress>,
) -> Result<GenerateAudioResult, String> {
    run_audio_generation(
        &state,
        &input.job_id,
        input.voice,
        input.speed,
        HashSet::from([input.segment_id]),
        on_progress,
    )
    .await
}

async fn run_audio_generation(
    state: &HostState,
    job_id: &str,
    voice: Option<String>,
    speed: f32,
    regenerate_segment_ids: HashSet<String>,
    on_progress: tauri::ipc::Channel<AudioGenerationProgress>,
) -> Result<GenerateAudioResult, String> {
    let cancellation = AudioCancellation::new();
    let _active = ActiveJobGuard::register(
        &state.active_audio_generations,
        job_id,
        cancellation.clone(),
        "audio generation",
    )?;
    let provider = audio_provider();
    let request = AudioGenerationRequest {
        voice: voice
            .filter(|voice| !voice.trim().is_empty())
            .unwrap_or_else(|| provider.default_voice().into()),
        speed,
        regenerate_segment_ids,
    };
    AudioGenerationService::new(Arc::clone(&state.service), provider)
        .generate(job_id, &request, &cancellation, |event| {
            let _ = on_progress.send(event);
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn cancel_audio_generation(state: State<'_, HostState>, job_id: String) -> Result<(), String> {
    let active = state
        .active_audio_generations
        .lock()
        .expect("active job lock poisoned");
    let cancellation = active
        .get(&job_id)
        .ok_or_else(|| format!("no audio generation is active for job {job_id}"))?;
    cancellation.cancel();
    Ok(())
}

#[tauri::command]
pub fn audio_asset(
    state: State<'_, HostState>,
    artifact_id: String,
) -> Result<tauri::ipc::Response, String> {
    state
        .service
        .read_binary_artifact(&artifact_id)
        .map(tauri::ipc::Response::new)
        .map_err(|error| error.to_string())
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
    let _active_run = ActiveJobGuard::register(
        &state.active_runs,
        &input.job_id,
        cancellation.clone(),
        "an agent run",
    )?;
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
    let active_runs = state.active_runs.lock().expect("active job lock poisoned");
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
        active_audio_generations: Mutex::new(HashMap::new()),
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

/// Kokoro's local OpenAI-compatible speech server is the primary audio
/// provider. Any future provider must deliver the same durable contract:
/// a container `durable_audio_duration_ms` can independently time.
fn audio_provider() -> KokoroProvider {
    KokoroProvider::new(kokoro_endpoint())
}

fn kokoro_endpoint() -> String {
    std::env::var("DIGEST_KOKORO_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:3000".into())
        .trim_end_matches('/')
        .to_owned()
}

fn default_audio_speed() -> f32 {
    1.0
}
