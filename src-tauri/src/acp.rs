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
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use thiserror::Error;

const AGENT_RUN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

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
}

impl AcpClient {
    pub fn new(service: Arc<DigestService>) -> Self {
        Self { service }
    }

    pub async fn run_once(
        &self,
        request: AgentRunRequest,
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
        let active_session = Arc::new(Mutex::new(None::<String>));
        let lifecycle_session = Arc::clone(&active_session);
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
                        kind: if response.stop_reason == StopReason::EndTurn {
                            NewAgentEventKind::SessionCompleted
                        } else {
                            NewAgentEventKind::SessionFailed
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
        let result = tokio::time::timeout(AGENT_RUN_TIMEOUT, agent_run).await;

        let result = match result {
            Ok(result) => result,
            Err(_) => {
                let message = format!(
                    "agent run exceeded the {} second limit",
                    AGENT_RUN_TIMEOUT.as_secs()
                );
                let session_id = active_session
                    .lock()
                    .expect("active session lock poisoned")
                    .clone();
                let _ = self.service.record_agent_event(NewAgentEvent {
                    job_id: failure_job_id.clone(),
                    session_id: session_id.clone().unwrap_or_else(|| "unavailable".into()),
                    kind: NewAgentEventKind::SessionFailed,
                    message: message.clone(),
                });
                let _ = self.service.finish_run_attempt(
                    &attempt.attempt_id,
                    AttemptStatus::Failed,
                    session_id.as_deref(),
                    Some(&message),
                );
                return Err(AcpClientError::Protocol(message));
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
        SessionUpdate::ToolCallUpdate(update) => match update.fields.status {
            Some(ToolCallStatus::Completed | ToolCallStatus::Failed) => {
                NewAgentEventKind::ToolCompleted
            }
            _ => NewAgentEventKind::ToolProgress,
        },
        _ => return None,
    };
    let message = serde_json::to_string(update).unwrap_or_else(|_| format!("{update:?}"));
    Some((kind, message))
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
