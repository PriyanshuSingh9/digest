use crate::{
    ArtifactKind, AttemptStatus, DigestError, DigestService, NewAgentEvent, NewAgentEventKind,
    StartRunAttempt,
};
use agent_client_protocol::schema::v1::{
    ContentBlock, McpServer, McpServerStdio, NewSessionRequest, PermissionOptionKind,
    PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigId, SessionConfigKind, SessionConfigOption,
    SessionConfigOptionCategory, SessionConfigSelectOptions, SessionConfigValueId,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, StopReason, TextContent,
    ToolCallStatus,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{AcpAgent, Agent, ConnectionTo};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    future::Future,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use thiserror::Error;
use tokio::sync::{watch, Notify};

const DEFAULT_AGENT_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum AgentProvider {
    OpenCode,
    Agy {
        adapter_command: PathBuf,
        #[serde(default)]
        adapter_args: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentLaunchSpec {
    pub command: PathBuf,
    pub args: Vec<String>,
}

impl AgentProvider {
    pub fn id(&self) -> &'static str {
        match self {
            Self::OpenCode => "open_code",
            Self::Agy { .. } => "agy",
        }
    }

    pub fn launch_spec(&self) -> AgentLaunchSpec {
        match self {
            Self::OpenCode => AgentLaunchSpec {
                command: "opencode".into(),
                args: vec!["acp".into()],
            },
            Self::Agy {
                adapter_command,
                adapter_args,
            } => AgentLaunchSpec {
                command: adapter_command.clone(),
                args: adapter_args.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpLaunchSpec {
    executable: PathBuf,
    data_dir: PathBuf,
    prefix_args: Vec<String>,
}

impl McpLaunchSpec {
    pub fn new(executable: PathBuf, data_dir: PathBuf) -> Result<Self, AcpClientError> {
        if !executable.is_absolute() {
            return Err(AcpClientError::InvalidConfiguration(
                "Digest MCP executable path must be absolute".into(),
            ));
        }
        if !data_dir.is_absolute() {
            return Err(AcpClientError::InvalidConfiguration(
                "Digest data directory must be absolute".into(),
            ));
        }
        Ok(Self {
            executable,
            data_dir,
            prefix_args: Vec::new(),
        })
    }

    pub fn with_prefix_args(mut self, prefix_args: Vec<String>) -> Self {
        self.prefix_args = prefix_args;
        self
    }

    pub fn acp_server(&self) -> McpServer {
        let mut args = self.prefix_args.clone();
        args.extend([
            "--data-dir".into(),
            self.data_dir.to_string_lossy().into_owned(),
        ]);
        McpServer::Stdio(McpServerStdio::new("digest", &self.executable).args(args))
    }
}

#[derive(Clone, Debug)]
pub struct AgentRunRequest {
    pub job_id: String,
    pub provider: AgentProvider,
    pub cwd: PathBuf,
    pub prompt: String,
    pub digest_mcp: McpLaunchSpec,
    pub permission_policy: PermissionPolicy,
    /// Optional harness model, e.g. `opencode/muse-spark-1.3-contributor-free`.
    /// Applied through `session/set_config_option` after `session/new` when the
    /// harness advertises a model selector. `None` keeps the harness default.
    pub model: Option<String>,
}

/// One selectable harness model, as advertised by the agent itself through its
/// `session/new` config options. The `id` is the wire value; `name` is display
/// text. No Digest-side catalog is kept, so this list can never go stale.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModel {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// Whether this is the harness's current default for a fresh session.
    pub current: bool,
}

/// The model selectors among a harness's advertised session config options.
fn model_select_options(options: &[SessionConfigOption]) -> Vec<&SessionConfigOption> {
    options
        .iter()
        .filter(|option| {
            option.category == Some(SessionConfigOptionCategory::Model)
                && matches!(option.kind, SessionConfigKind::Select(_))
        })
        .collect()
}

fn flat_select_options(
    option: &SessionConfigOption,
) -> Vec<(String, String, Option<String>, bool)> {
    let SessionConfigKind::Select(select) = &option.kind else {
        return Vec::new();
    };
    let listed = match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => options.clone(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.clone())
            .collect(),
        _ => Vec::new(),
    };
    listed
        .into_iter()
        .map(|entry| {
            let current = entry.value.to_string() == select.current_value.to_string();
            (
                entry.value.to_string(),
                entry.name,
                entry.description,
                current,
            )
        })
        .collect()
}

/// Project advertised config options down to the display list the UI shows.
fn agent_models(options: &[SessionConfigOption]) -> Vec<AgentModel> {
    model_select_options(options)
        .into_iter()
        .flat_map(flat_select_options)
        .map(|(id, name, description, current)| AgentModel {
            id,
            name,
            description,
            current,
        })
        .collect()
}

/// Resolve a requested model against what the harness advertised. Returns the
/// config option to set and the value to set it to. The error names every value
/// the harness accepts, so the caller can correct the request instead of
/// guessing.
fn resolve_model_option(
    options: &[SessionConfigOption],
    requested: &str,
) -> Result<(SessionConfigId, SessionConfigValueId), String> {
    let selectors = model_select_options(options);
    if selectors.is_empty() {
        let advertised: Vec<String> = options
            .iter()
            .map(|option| option.id.to_string())
            .collect();
        return Err(format!(
            "harness does not advertise a model selector (advertised options: {}); \
             run without a model to use the harness default",
            if advertised.is_empty() {
                "none".into()
            } else {
                advertised.join(", ")
            }
        ));
    }
    for selector in &selectors {
        for (value, _, _, _) in flat_select_options(selector) {
            if value == requested {
                return Ok((
                    selector.id.clone(),
                    SessionConfigValueId::new(requested),
                ));
            }
        }
    }
    let accepted: Vec<String> = selectors
        .iter()
        .flat_map(|selector| flat_select_options(selector))
        .map(|(value, _, _, _)| value)
        .collect();
    Err(format!(
        "harness does not offer model `{requested}`; accepted values: {}",
        accepted.join(", ")
    ))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PermissionPolicy {
    #[default]
    Deny,
    AllowOnce,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunResult {
    pub session_id: String,
    pub stop_reason: String,
    /// Required work still missing when the session ended, empty when the
    /// agent produced both the analysis and the narration plan.
    pub missing_work: Vec<String>,
    /// Follow-up prompts Digest sent after the agent ended early.
    pub reminder_rounds: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct AgentRunSupervision {
    pub inactivity_timeout: Duration,
    pub max_runtime: Option<Duration>,
}

impl Default for AgentRunSupervision {
    fn default() -> Self {
        Self {
            inactivity_timeout: DEFAULT_AGENT_INACTIVITY_TIMEOUT,
            max_runtime: None,
        }
    }
}

#[derive(Clone, Default)]
pub struct AgentRunCancellation {
    cancelled: Arc<AtomicBool>,
    notification: Arc<Notify>,
}

impl AgentRunCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notification.notify_waiters();
    }

    async fn cancelled(&self) {
        if self.cancelled.load(Ordering::Acquire) {
            return;
        }
        let notified = self.notification.notified();
        if self.cancelled.load(Ordering::Acquire) {
            return;
        }
        notified.await;
    }
}

#[derive(Clone)]
struct ActivityRecorder {
    generation: watch::Sender<u64>,
}

impl ActivityRecorder {
    fn record(&self) {
        self.generation.send_modify(|generation| {
            *generation = generation.wrapping_add(1);
        });
    }
}

fn activity_channel() -> (ActivityRecorder, watch::Receiver<u64>) {
    let (generation, receiver) = watch::channel(0);
    (ActivityRecorder { generation }, receiver)
}

#[derive(Debug)]
enum SupervisionOutcome<T> {
    Finished(T),
    Cancelled,
    Inactive,
    MaxRuntime,
}

async fn supervise_agent_run<F>(
    future: F,
    mut activity: watch::Receiver<u64>,
    cancellation: AgentRunCancellation,
    supervision: AgentRunSupervision,
) -> SupervisionOutcome<F::Output>
where
    F: Future,
{
    let maximum_deadline = supervision
        .max_runtime
        .map(|duration| tokio::time::Instant::now() + duration);
    let inactivity = tokio::time::sleep(supervision.inactivity_timeout);
    tokio::pin!(future);
    tokio::pin!(inactivity);

    loop {
        tokio::select! {
            output = &mut future => return SupervisionOutcome::Finished(output),
            _ = cancellation.cancelled() => return SupervisionOutcome::Cancelled,
            changed = activity.changed() => {
                if changed.is_ok() {
                    inactivity
                        .as_mut()
                        .reset(tokio::time::Instant::now() + supervision.inactivity_timeout);
                }
            }
            _ = &mut inactivity => return SupervisionOutcome::Inactive,
            _ = async {
                match maximum_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            } => return SupervisionOutcome::MaxRuntime,
        }
    }
}

#[derive(Debug, Error)]
pub enum AcpClientError {
    #[error("invalid ACP configuration: {0}")]
    InvalidConfiguration(String),
    #[error("ACP session failed: {0}")]
    Protocol(String),
    #[error("could not persist ACP event: {0}")]
    Persistence(String),
}

#[derive(Clone)]
pub struct AcpClient {
    service: Arc<DigestService>,
    supervision: AgentRunSupervision,
}

impl AcpClient {
    pub fn new(service: Arc<DigestService>) -> Self {
        Self {
            service,
            supervision: AgentRunSupervision::default(),
        }
    }

    pub fn with_supervision(mut self, supervision: AgentRunSupervision) -> Self {
        self.supervision = supervision;
        self
    }

    pub async fn run_once(
        &self,
        request: AgentRunRequest,
    ) -> Result<AgentRunResult, AcpClientError> {
        self.run_once_with_cancellation(request, AgentRunCancellation::new())
            .await
    }

    /// Ask a harness which models it offers, without starting a run. Spawns the
    /// agent, runs `initialize` + `session/new`, and reads the model selectors
    /// from the advertised config options. An empty list means the harness does
    /// not offer model selection. The child process tree is torn down when the
    /// connection closes.
    pub async fn list_agent_models(
        &self,
        provider: &AgentProvider,
        cwd: &std::path::Path,
    ) -> Result<Vec<AgentModel>, AcpClientError> {
        if !cwd.is_absolute() {
            return Err(AcpClientError::InvalidConfiguration(
                "ACP working directory must be absolute".into(),
            ));
        }
        if let AgentProvider::Agy { adapter_command, .. } = provider {
            if adapter_command.as_os_str().is_empty() {
                return Err(AcpClientError::InvalidConfiguration(
                    "agy adapter command is required".into(),
                ));
            }
        }
        let launch = provider.launch_spec();
        let mut command = vec![launch.command.to_string_lossy().into_owned()];
        command.extend(launch.args);
        let agent = AcpAgent::from_args(command)
            .map_err(|error| AcpClientError::Protocol(error.to_string()))?;
        let probe = agent_client_protocol::Client
            .builder()
            .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                connection
                    .send_request(agent_client_protocol::schema::v1::InitializeRequest::new(
                        ProtocolVersion::V1,
                    ))
                    .block_task()
                    .await?;
                let new_session = connection
                    .send_request(NewSessionRequest::new(cwd).mcp_servers(vec![]))
                    .block_task()
                    .await?;
                Ok(agent_models(
                    new_session.config_options.as_deref().unwrap_or(&[]),
                ))
            });
        tokio::time::timeout(Duration::from_secs(60), probe)
            .await
            .map_err(|_| {
                AcpClientError::Protocol(
                    "harness did not answer the model probe within 60 seconds".into(),
                )
            })?
            .map_err(|error| AcpClientError::Protocol(error.to_string()))
    }

    pub async fn run_once_with_cancellation(
        &self,
        request: AgentRunRequest,
        cancellation: AgentRunCancellation,
    ) -> Result<AgentRunResult, AcpClientError> {
        validate_request(&request)?;
        let attempt = self
            .service
            .start_run_attempt(StartRunAttempt {
                job_id: request.job_id.clone(),
                provider: request.provider.id().into(),
            })
            .map_err(|error| AcpClientError::Persistence(error.to_string()))?;
        let launch = request.provider.launch_spec();
        let mut command = vec![launch.command.to_string_lossy().into_owned()];
        command.extend(launch.args);
        let agent = match AcpAgent::from_args(command) {
            Ok(agent) => agent,
            Err(error) => {
                let message = error.to_string();
                let _ = self.service.finish_run_attempt(
                    &attempt.attempt_id,
                    AttemptStatus::Failed,
                    None,
                    Some(&message),
                );
                return Err(AcpClientError::Protocol(message));
            }
        };

        let notification_service = Arc::clone(&self.service);
        let notification_job_id = request.job_id.clone();
        let failure_job_id = request.job_id.clone();
        let persistence_error = Arc::new(Mutex::new(None::<String>));
        let notification_error = Arc::clone(&persistence_error);
        let active_tools = Arc::new(Mutex::new(HashMap::<String, String>::new()));
        let notification_tools = Arc::clone(&active_tools);
        let active_session = Arc::new(Mutex::new(None::<String>));
        let lifecycle_session = Arc::clone(&active_session);
        let (activity, activity_receiver) = activity_channel();
        let notification_activity = activity.clone();
        let permission_activity = activity.clone();
        let lifecycle_activity = activity.clone();
        let session_request =
            NewSessionRequest::new(&request.cwd).mcp_servers(vec![request.digest_mcp.acp_server()]);
        let prompt = request.prompt;
        let job_id = request.job_id;
        let permission_policy = request.permission_policy;
        let requested_model = request.model;
        let lifecycle_service = Arc::clone(&self.service);

        let agent_run = agent_client_protocol::Client
            .builder()
            .on_receive_notification(
                async move |notification: SessionNotification, _context| {
                    notification_activity.record();
                    track_active_tools(&notification.update, &notification_tools);
                    if let Some((kind, message)) = canonical_event(&notification.update) {
                        if let Err(error) = notification_service.record_agent_event(NewAgentEvent {
                            job_id: notification_job_id.clone(),
                            session_id: notification.session_id.to_string(),
                            kind,
                            message,
                        }) {
                            *notification_error
                                .lock()
                                .expect("event error lock poisoned") = Some(error.to_string());
                        }
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest, responder, _connection| {
                    permission_activity.record();
                    let outcome = if permission_policy == PermissionPolicy::AllowOnce {
                        request
                            .options
                            .iter()
                            .find(|option| option.kind == PermissionOptionKind::AllowOnce)
                            .map(|option| {
                                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                    option.option_id.clone(),
                                ))
                            })
                            .unwrap_or(RequestPermissionOutcome::Cancelled)
                    } else {
                        RequestPermissionOutcome::Cancelled
                    };
                    responder.respond(RequestPermissionResponse::new(outcome))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                connection
                    .send_request(agent_client_protocol::schema::v1::InitializeRequest::new(
                        ProtocolVersion::V1,
                    ))
                    .block_task()
                    .await?;
                let new_session = connection
                    .send_request(session_request)
                    .block_task()
                    .await?;
                lifecycle_activity.record();
                let session_id = new_session.session_id;
                let applied_model = if let Some(model) = requested_model {
                    let advertised = new_session.config_options.as_deref().unwrap_or(&[]);
                    let (config_id, value) = resolve_model_option(advertised, &model).map_err(
                        |message| {
                            agent_client_protocol::Error::internal_error().data(message)
                        },
                    )?;
                    connection
                        .send_request(SetSessionConfigOptionRequest::new(
                            session_id.clone(),
                            config_id,
                            value,
                        ))
                        .block_task()
                        .await?;
                    Some(model)
                } else {
                    None
                };
                *lifecycle_session
                    .lock()
                    .expect("active session lock poisoned") = Some(session_id.to_string());
                lifecycle_service
                    .record_agent_event(NewAgentEvent {
                        job_id: job_id.clone(),
                        session_id: session_id.to_string(),
                        kind: NewAgentEventKind::SessionStarted,
                        message: match &applied_model {
                            Some(model) => format!("ACP session started with model {model}"),
                            None => "ACP session started".into(),
                        },
                    })
                    .map_err(|error| {
                        agent_client_protocol::Error::internal_error().data(error.to_string())
                    })?;
                // The agent may end its turn having written nothing, as observed
                // when a small model rambles through a long article and stops.
                // After each agent-ended turn, check the job for its required
                // artifacts and, while rounds remain, hand the agent its exact
                // outstanding steps instead of accepting an empty completion.
                let mut prompt_text = prompt;
                let mut reminder_rounds: u32 = 0;
                let mut missing_work: Vec<String> = Vec::new();
                let response = loop {
                    let response = connection
                        .send_request(PromptRequest::new(
                            session_id.clone(),
                            vec![ContentBlock::Text(TextContent::new(prompt_text))],
                        ))
                        .block_task()
                        .await?;
                    lifecycle_activity.record();
                    lifecycle_service
                        .record_agent_event(NewAgentEvent {
                            job_id: job_id.clone(),
                            session_id: session_id.to_string(),
                            kind: match response.stop_reason {
                                StopReason::EndTurn => NewAgentEventKind::SessionCompleted,
                                StopReason::Cancelled => NewAgentEventKind::SessionCancelled,
                                _ => NewAgentEventKind::SessionFailed,
                            },
                            message: format!(
                                "ACP session stopped: {:?}",
                                response.stop_reason
                            ),
                        })
                        .map_err(|error| {
                            agent_client_protocol::Error::internal_error().data(error.to_string())
                        })?;
                    if !is_resumable_stop(&response.stop_reason) {
                        break response;
                    }
                    missing_work = missing_work_artifacts(&lifecycle_service, &job_id)
                        .map_err(|error| {
                            agent_client_protocol::Error::internal_error().data(error.to_string())
                        })?;
                    if missing_work.is_empty() || reminder_rounds >= MAX_REMINDER_ROUNDS
                    {
                        break response;
                    }
                    reminder_rounds += 1;
                    prompt_text = reminder_prompt(reminder_rounds, &missing_work);
                    lifecycle_activity.record();
                    lifecycle_service
                        .record_agent_event(NewAgentEvent {
                            job_id: job_id.clone(),
                            session_id: session_id.to_string(),
                            kind: NewAgentEventKind::SessionReminded,
                            message: prompt_text.clone(),
                        })
                        .map_err(|error| {
                            agent_client_protocol::Error::internal_error().data(error.to_string())
                        })?;
                };
                Ok(AgentRunResult {
                    session_id: session_id.to_string(),
                    stop_reason: stop_reason_name(response.stop_reason).into(),
                    missing_work,
                    reminder_rounds,
                })
            });
        let result =
            match supervise_agent_run(agent_run, activity_receiver, cancellation, self.supervision)
                .await
            {
                SupervisionOutcome::Finished(result) => result,
                SupervisionOutcome::Cancelled => {
                    let message = "agent run cancelled by user";
                    let session_id = active_session
                        .lock()
                        .expect("active session lock poisoned")
                        .clone();
                    record_interrupted_tools(
                        &self.service,
                        &failure_job_id,
                        session_id.as_deref(),
                        &active_tools,
                        message,
                    );
                    let _ = self.service.record_agent_event(NewAgentEvent {
                        job_id: failure_job_id,
                        session_id: session_id.clone().unwrap_or_else(|| "unavailable".into()),
                        kind: NewAgentEventKind::SessionCancelled,
                        message: message.into(),
                    });
                    self.service
                        .finish_run_attempt(
                            &attempt.attempt_id,
                            AttemptStatus::Cancelled,
                            session_id.as_deref(),
                            None,
                        )
                        .map_err(|error| AcpClientError::Persistence(error.to_string()))?;
                    return Ok(AgentRunResult {
                        session_id: session_id.unwrap_or_else(|| "unavailable".into()),
                        stop_reason: "cancelled".into(),
                        missing_work: Vec::new(),
                        reminder_rounds: 0,
                    });
                }
                SupervisionOutcome::Inactive => {
                    let message = format!(
                        "agent run produced no activity for {} seconds",
                        self.supervision.inactivity_timeout.as_secs()
                    );
                    return fail_interrupted_run(
                        &self.service,
                        &attempt.attempt_id,
                        &failure_job_id,
                        &active_session,
                        &active_tools,
                        message,
                    );
                }
                SupervisionOutcome::MaxRuntime => {
                    let seconds = self
                        .supervision
                        .max_runtime
                        .map(|duration| duration.as_secs())
                        .unwrap_or_default();
                    let message =
                        format!("agent run exceeded the configured {seconds} second limit");
                    return fail_interrupted_run(
                        &self.service,
                        &attempt.attempt_id,
                        &failure_job_id,
                        &active_session,
                        &active_tools,
                        message,
                    );
                }
            };

        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let session_id = active_session
                    .lock()
                    .expect("active session lock poisoned")
                    .clone()
                    .unwrap_or_else(|| "unavailable".into());
                record_interrupted_tools(
                    &self.service,
                    &failure_job_id,
                    (session_id != "unavailable").then_some(session_id.as_str()),
                    &active_tools,
                    "ACP session failed",
                );
                let _ = self.service.record_agent_event(NewAgentEvent {
                    job_id: failure_job_id,
                    session_id: session_id.clone(),
                    kind: NewAgentEventKind::SessionFailed,
                    message: error.to_string(),
                });
                let message = error.to_string();
                let _ = self.service.finish_run_attempt(
                    &attempt.attempt_id,
                    AttemptStatus::Failed,
                    (session_id != "unavailable").then_some(session_id.as_str()),
                    Some(&message),
                );
                return Err(AcpClientError::Protocol(error.to_string()));
            }
        };

        if let Some(error) = persistence_error
            .lock()
            .expect("event error lock poisoned")
            .take()
        {
            let _ = self.service.finish_run_attempt(
                &attempt.attempt_id,
                AttemptStatus::Failed,
                Some(&result.session_id),
                Some(&error),
            );
            return Err(AcpClientError::Persistence(error));
        }
        let status = match result.stop_reason.as_str() {
            "end_turn" | "max_tokens" | "max_turn_requests"
                if !result.missing_work.is_empty() =>
            {
                AttemptStatus::Incomplete
            }
            "end_turn" => AttemptStatus::Completed,
            "cancelled" => AttemptStatus::Cancelled,
            _ => AttemptStatus::Failed,
        };
        let failure_message;
        let error = match status {
            AttemptStatus::Failed => Some(result.stop_reason.as_str()),
            AttemptStatus::Incomplete => {
                failure_message = format!(
                    "agent ended without completing the work after {} reminder(s): missing {}. \
                     Re-run to continue from the existing artifacts.",
                    result.reminder_rounds,
                    result.missing_work.join("; "),
                );
                Some(failure_message.as_str())
            }
            _ => None,
        };
        self.service
            .finish_run_attempt(
                &attempt.attempt_id,
                status,
                Some(&result.session_id),
                error,
            )
            .map_err(|error| AcpClientError::Persistence(error.to_string()))?;
        Ok(result)
    }
}

fn fail_interrupted_run(
    service: &DigestService,
    attempt_id: &str,
    job_id: &str,
    active_session: &Mutex<Option<String>>,
    active_tools: &Mutex<HashMap<String, String>>,
    message: String,
) -> Result<AgentRunResult, AcpClientError> {
    let session_id = active_session
        .lock()
        .expect("active session lock poisoned")
        .clone();
    record_interrupted_tools(
        service,
        job_id,
        session_id.as_deref(),
        active_tools,
        &message,
    );
    let _ = service.record_agent_event(NewAgentEvent {
        job_id: job_id.into(),
        session_id: session_id.clone().unwrap_or_else(|| "unavailable".into()),
        kind: NewAgentEventKind::SessionFailed,
        message: message.clone(),
    });
    let _ = service.finish_run_attempt(
        attempt_id,
        AttemptStatus::Failed,
        session_id.as_deref(),
        Some(&message),
    );
    Err(AcpClientError::Protocol(message))
}

fn record_interrupted_tools(
    service: &DigestService,
    job_id: &str,
    session_id: Option<&str>,
    active_tools: &Mutex<HashMap<String, String>>,
    reason: &str,
) {
    let tools = std::mem::take(
        &mut *active_tools
            .lock()
            .expect("active tool calls lock poisoned"),
    );
    for (tool_call_id, title) in tools {
        let _ = service.record_agent_event(NewAgentEvent {
            job_id: job_id.into(),
            session_id: session_id.unwrap_or("unavailable").into(),
            kind: NewAgentEventKind::ToolFailed,
            message: format!("Tool call `{title}` ({tool_call_id}) was interrupted: {reason}"),
        });
    }
}

fn track_active_tools(update: &SessionUpdate, active_tools: &Mutex<HashMap<String, String>>) {
    let mut active_tools = active_tools
        .lock()
        .expect("active tool calls lock poisoned");
    match update {
        SessionUpdate::ToolCall(tool_call) => {
            active_tools.insert(tool_call.tool_call_id.to_string(), tool_call.title.clone());
        }
        SessionUpdate::ToolCallUpdate(update) => {
            let terminal = matches!(
                update.fields.status,
                Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
            ) || update
                .fields
                .title
                .as_deref()
                .is_some_and(|title| title.trim().eq_ignore_ascii_case("Invalid Tool"));
            if terminal {
                active_tools.remove(&update.tool_call_id.to_string());
            } else if let Some(title) = &update.fields.title {
                if let Some(active_title) = active_tools.get_mut(&update.tool_call_id.to_string()) {
                    *active_title = title.clone();
                }
            }
        }
        _ => {}
    }
}

fn validate_request(request: &AgentRunRequest) -> Result<(), AcpClientError> {
    require_value("job ID", &request.job_id)?;
    require_value("prompt", &request.prompt)?;
    if !request.cwd.is_absolute() {
        return Err(AcpClientError::InvalidConfiguration(
            "ACP working directory must be absolute".into(),
        ));
    }
    if let AgentProvider::Agy {
        adapter_command, ..
    } = &request.provider
    {
        if adapter_command.as_os_str().is_empty() {
            return Err(AcpClientError::InvalidConfiguration(
                "agy adapter command is required".into(),
            ));
        }
        if request.model.as_ref().is_some_and(|model| !model.trim().is_empty()) {
            return Err(AcpClientError::InvalidConfiguration(
                "model selection is only supported for the OpenCode provider; \
                 the agy adapter owns its own model through its adapter arguments"
                    .into(),
            ));
        }
    }
    Ok(())
}

fn require_value(name: &str, value: &str) -> Result<(), AcpClientError> {
    if value.trim().is_empty() {
        Err(AcpClientError::InvalidConfiguration(format!(
            "{name} is required"
        )))
    } else {
        Ok(())
    }
}

fn canonical_event(update: &SessionUpdate) -> Option<(NewAgentEventKind, String)> {
    let kind = match update {
        SessionUpdate::AgentMessageChunk(_) => NewAgentEventKind::AgentMessage,
        SessionUpdate::AgentThoughtChunk(_) => NewAgentEventKind::AgentThinking,
        SessionUpdate::ToolCall(_) => NewAgentEventKind::ToolStarted,
        SessionUpdate::ToolCallUpdate(update) => {
            tool_update_event_kind(update.fields.status, update.fields.title.as_deref())
        }
        _ => return None,
    };
    let message = serde_json::to_string(update).unwrap_or_else(|_| format!("{update:?}"));
    Some((kind, message))
}

fn tool_update_event_kind(
    status: Option<ToolCallStatus>,
    title: Option<&str>,
) -> NewAgentEventKind {
    if status == Some(ToolCallStatus::Failed)
        || title.is_some_and(|title| title.trim().eq_ignore_ascii_case("Invalid Tool"))
    {
        NewAgentEventKind::ToolFailed
    } else if status == Some(ToolCallStatus::Completed) {
        NewAgentEventKind::ToolCompleted
    } else {
        NewAgentEventKind::ToolProgress
    }
}

/// Follow-up prompts Digest sends when the agent ends its turn with required
/// work still missing. Bounded so a stuck agent cannot loop forever; each
/// round is recorded as a `session_reminded` event for inspection.
const MAX_REMINDER_ROUNDS: u32 = 2;

/// Turns the agent may resume from: it ended on its own side, whether
/// cleanly, cut off by a token budget, or stopped between turns. Refusals and
/// cancellations are terminal and never reminded.
fn is_resumable_stop(reason: &StopReason) -> bool {
    matches!(
        reason,
        StopReason::EndTurn | StopReason::MaxTokens | StopReason::MaxTurnRequests
    )
}

/// Human steps for the work no artifact covers yet. Pure over artifact
/// presence so the wording is unit-testable without a database.
fn describe_missing_work(has_analysis: bool, has_narration: bool) -> Vec<String> {
    let mut missing = Vec::new();
    if !has_analysis {
        missing.push(
            "the analysis is missing: call write_analysis with the central argument and \
             findings citing source block ids before doing anything else"
                .into(),
        );
    }
    if !has_narration {
        missing.push(
            "the narration plan is missing: call write_narration_plan with source-grounded \
             segments covering every source block before ending the turn"
                .into(),
        );
    }
    missing
}

fn missing_work_artifacts(
    service: &DigestService,
    job_id: &str,
) -> Result<Vec<String>, DigestError> {
    let has_analysis = service
        .latest_artifact(job_id, ArtifactKind::Analysis)?
        .is_some();
    let has_narration = service
        .latest_artifact(job_id, ArtifactKind::NarrationPlan)?
        .is_some();
    Ok(describe_missing_work(has_analysis, has_narration))
}

fn reminder_prompt(round: u32, missing: &[String]) -> String {
    let steps = missing
        .iter()
        .enumerate()
        .map(|(index, step)| format!("{}. {}", index + 1, step))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "You ended the turn with required work still missing (reminder {round} of \
         {MAX_REMINDER_ROUNDS}). Complete exactly these steps now, then end the turn:\n{steps}\n\
         Do not end the turn until the missing artifacts exist."
    )
}

fn stop_reason_name(reason: StopReason) -> &'static str {
    match reason {
        StopReason::EndTurn => "end_turn",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurnRequests => "max_turn_requests",
        StopReason::Refusal => "refusal",
        StopReason::Cancelled => "cancelled",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::SessionConfigSelectOption;
    use std::future::pending;

    #[test]
    fn missing_work_names_each_absent_artifact() {
        assert!(describe_missing_work(true, true).is_empty());

        let missing = describe_missing_work(false, false);
        assert_eq!(missing.len(), 2);
        assert!(missing[0].contains("write_analysis"), "{missing:?}");
        assert!(missing[1].contains("write_narration_plan"), "{missing:?}");

        let missing = describe_missing_work(true, false);
        assert_eq!(missing.len(), 1);
        assert!(missing[0].contains("write_narration_plan"), "{missing:?}");
    }

    #[test]
    fn reminder_prompt_carries_the_steps_and_the_round_budget() {
        let missing = describe_missing_work(false, true);
        let prompt = reminder_prompt(1, &missing);
        assert!(prompt.contains("reminder 1 of 2"), "{prompt}");
        assert!(prompt.contains("write_analysis"), "{prompt}");
        assert!(
            prompt.contains("Do not end the turn until the missing artifacts exist."),
            "{prompt}"
        );
    }

    #[test]
    fn only_agent_ended_turns_are_resumable() {
        assert!(is_resumable_stop(&StopReason::EndTurn));
        assert!(is_resumable_stop(&StopReason::MaxTokens));
        assert!(is_resumable_stop(&StopReason::MaxTurnRequests));
        assert!(!is_resumable_stop(&StopReason::Refusal));
        assert!(!is_resumable_stop(&StopReason::Cancelled));
    }

    fn model_selector() -> SessionConfigOption {
        SessionConfigOption::select(
            "model",
            "Model",
            "opencode/big-pickle",
            vec![
                SessionConfigSelectOption::new("opencode/big-pickle", "Big Pickle"),
                SessionConfigSelectOption::new("opencode/space-bunny-free", "Space Bunny"),
            ],
        )
        .category(SessionConfigOptionCategory::Model)
    }

    #[test]
    fn requested_model_resolves_against_advertised_values() {
        let (config_id, value) =
            resolve_model_option(&[model_selector()], "opencode/space-bunny-free")
                .expect("advertised model must resolve");
        assert_eq!(config_id.to_string(), "model");
        assert_eq!(value.to_string(), "opencode/space-bunny-free");
    }

    #[test]
    fn unknown_model_names_every_accepted_value() {
        let error = resolve_model_option(&[model_selector()], "opencode/nope")
            .expect_err("unadvertised model must fail");
        assert!(error.contains("opencode/big-pickle"), "{error}");
        assert!(error.contains("opencode/space-bunny-free"), "{error}");
    }

    #[test]
    fn harness_without_model_selector_is_a_clear_error() {
        let error = resolve_model_option(&[], "opencode/big-pickle")
            .expect_err("missing selector must fail");
        assert!(error.contains("does not advertise"), "{error}");
        assert!(error.contains("harness default"), "{error}");
    }

    #[test]
    fn non_model_selectors_are_ignored() {
        let mode = SessionConfigOption::select(
            "mode",
            "Session Mode",
            "build",
            vec![SessionConfigSelectOption::new("build", "build")],
        )
        .category(SessionConfigOptionCategory::Mode);
        let models = agent_models(&[mode, model_selector()]);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "opencode/big-pickle");
        assert_eq!(models[0].name, "Big Pickle");
    }

    #[test]
    fn agy_with_model_is_rejected_before_any_subprocess_starts() {
        let request = AgentRunRequest {
            job_id: "job-1".into(),
            provider: AgentProvider::Agy {
                adapter_command: "/bin/agy".into(),
                adapter_args: Vec::new(),
            },
            cwd: "/tmp".into(),
            prompt: "prompt".into(),
            digest_mcp: McpLaunchSpec::new("/bin/digest".into(), "/tmp/data".into())
                .expect("absolute fixture paths"),
            permission_policy: PermissionPolicy::Deny,
            model: Some("opencode/big-pickle".into()),
        };
        let error = validate_request(&request).expect_err("agy plus model must fail");
        assert!(error.to_string().contains("OpenCode"), "{error}");
    }

    #[test]
    fn invalid_or_failed_tool_updates_are_failures_even_when_acp_says_completed() {
        assert_eq!(
            tool_update_event_kind(Some(ToolCallStatus::Completed), Some("Invalid Tool")),
            NewAgentEventKind::ToolFailed
        );
        assert_eq!(
            tool_update_event_kind(Some(ToolCallStatus::Failed), Some("digest_write_analysis")),
            NewAgentEventKind::ToolFailed
        );
        assert_eq!(
            tool_update_event_kind(
                Some(ToolCallStatus::Completed),
                Some("digest_write_analysis")
            ),
            NewAgentEventKind::ToolCompleted
        );
    }

    #[tokio::test(start_paused = true)]
    async fn continuous_agent_activity_outlives_the_old_wall_clock_limit() {
        let supervision = AgentRunSupervision {
            inactivity_timeout: Duration::from_secs(60),
            max_runtime: None,
        };
        let cancellation = AgentRunCancellation::new();
        let (activity, activity_rx) = activity_channel();
        let supervised = supervise_agent_run(
            pending::<Result<(), ()>>(),
            activity_rx,
            cancellation,
            supervision,
        );
        tokio::pin!(supervised);

        assert!(tokio::time::timeout(Duration::ZERO, &mut supervised)
            .await
            .is_err());
        for _ in 0..11 {
            tokio::time::advance(Duration::from_secs(30)).await;
            activity.record();
            assert!(tokio::time::timeout(Duration::ZERO, &mut supervised)
                .await
                .is_err());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn inactive_and_cancelled_runs_have_distinct_outcomes() {
        let supervision = AgentRunSupervision {
            inactivity_timeout: Duration::from_secs(60),
            max_runtime: None,
        };
        let (_activity, activity_rx) = activity_channel();
        let inactive = supervise_agent_run(
            pending::<Result<(), ()>>(),
            activity_rx,
            AgentRunCancellation::new(),
            supervision,
        );
        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(matches!(inactive.await, SupervisionOutcome::Inactive));

        let cancellation = AgentRunCancellation::new();
        let cancellation_handle = cancellation.clone();
        let (_activity, activity_rx) = activity_channel();
        let cancelled = supervise_agent_run(
            pending::<Result<(), ()>>(),
            activity_rx,
            cancellation,
            supervision,
        );
        cancellation_handle.cancel();
        assert!(matches!(cancelled.await, SupervisionOutcome::Cancelled));
    }

    #[tokio::test(start_paused = true)]
    async fn configured_maximum_runtime_is_optional_and_distinct_from_inactivity() {
        let supervision = AgentRunSupervision {
            inactivity_timeout: Duration::from_secs(600),
            max_runtime: Some(Duration::from_secs(120)),
        };
        let (_activity, activity_rx) = activity_channel();

        let outcome = supervise_agent_run(
            pending::<Result<(), ()>>(),
            activity_rx,
            AgentRunCancellation::new(),
            supervision,
        )
        .await;

        assert!(matches!(outcome, SupervisionOutcome::MaxRuntime));
    }

    #[test]
    fn interrupted_tool_calls_are_persisted_as_failures() {
        let directory = tempfile::tempdir().expect("create temporary data directory");
        let service = DigestService::open(directory.path()).expect("open Digest service");
        let active_tools = Mutex::new(HashMap::from([(
            "call-1".into(),
            "digest_write_analysis".into(),
        )]));

        record_interrupted_tools(
            &service,
            "job-1",
            Some("session-1"),
            &active_tools,
            "agent run cancelled by user",
        );

        let events = service
            .list_agent_events("job-1")
            .expect("read persisted events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, NewAgentEventKind::ToolFailed);
        assert!(events[0].message.contains("digest_write_analysis"));
        assert!(events[0].message.contains("cancelled by user"));
        assert!(active_tools.lock().expect("active tools lock").is_empty());
    }
}
