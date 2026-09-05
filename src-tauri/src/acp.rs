use crate::{AttemptStatus, DigestService, NewAgentEvent, NewAgentEventKind, StartRunAttempt};
use agent_client_protocol::schema::v1::{
    ContentBlock, McpServer, McpServerStdio, NewSessionRequest, PermissionOptionKind,
    PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, StopReason, TextContent,
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
                *lifecycle_session
                    .lock()
                    .expect("active session lock poisoned") = Some(session_id.to_string());
                lifecycle_service
                    .record_agent_event(NewAgentEvent {
                        job_id: job_id.clone(),
                        session_id: session_id.to_string(),
                        kind: NewAgentEventKind::SessionStarted,
                        message: "ACP session started".into(),
                    })
                    .map_err(|error| {
                        agent_client_protocol::Error::internal_error().data(error.to_string())
                    })?;
                let response = connection
                    .send_request(PromptRequest::new(
                        session_id.clone(),
                        vec![ContentBlock::Text(TextContent::new(prompt))],
                    ))
                    .block_task()
                    .await?;
                lifecycle_service
                    .record_agent_event(NewAgentEvent {
                        job_id,
                        session_id: session_id.to_string(),
                        kind: match response.stop_reason {
                            StopReason::EndTurn => NewAgentEventKind::SessionCompleted,
                            StopReason::Cancelled => NewAgentEventKind::SessionCancelled,
                            _ => NewAgentEventKind::SessionFailed,
                        },
                        message: format!("ACP session stopped: {:?}", response.stop_reason),
                    })
                    .map_err(|error| {
                        agent_client_protocol::Error::internal_error().data(error.to_string())
                    })?;
                Ok(AgentRunResult {
                    session_id: session_id.to_string(),
                    stop_reason: stop_reason_name(response.stop_reason).into(),
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
            "end_turn" => AttemptStatus::Completed,
            "cancelled" => AttemptStatus::Cancelled,
            _ => AttemptStatus::Failed,
        };
        self.service
            .finish_run_attempt(
                &attempt.attempt_id,
                status,
                Some(&result.session_id),
                (status == AttemptStatus::Failed).then_some(result.stop_reason.as_str()),
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
    use std::future::pending;

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
