use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

const ARTIFACT_SCHEMA_VERSION: &str = "1.0";
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum DigestError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("artifact not found: {0}")]
    ArtifactNotFound(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDraft {
    pub job_id: String,
    pub article_id: String,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Analysis,
    SourceCapture,
    NormalizedArticle,
    ImageAsset,
    NarrationPlan,
}

impl ArtifactKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::SourceCapture => "source_capture",
            Self::NormalizedArticle => "normalized_article",
            Self::ImageAsset => "image_asset",
            Self::NarrationPlan => "narration_plan",
        }
    }

    fn parse(value: &str) -> Result<Self, DigestError> {
        match value {
            "analysis" => Ok(Self::Analysis),
            "source_capture" => Ok(Self::SourceCapture),
            "normalized_article" => Ok(Self::NormalizedArticle),
            "image_asset" => Ok(Self::ImageAsset),
            "narration_plan" => Ok(Self::NarrationPlan),
            other => Err(DigestError::InvalidInput(format!(
                "unknown artifact kind: {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactEnvelope {
    pub schema_version: String,
    pub artifact_id: String,
    pub job_id: String,
    pub kind: ArtifactKind,
    pub content_hash: String,
    pub created_at_ms: i64,
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewAgentEventKind {
    SessionStarted,
    AgentMessage,
    AgentThinking,
    ToolStarted,
    ToolProgress,
    ToolCompleted,
    ToolFailed,
    ArtifactCreated,
    SessionCompleted,
    SessionCancelled,
    SessionFailed,
}

impl NewAgentEventKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::SessionStarted => "session_started",
            Self::AgentMessage => "agent_message",
            Self::AgentThinking => "agent_thinking",
            Self::ToolStarted => "tool_started",
            Self::ToolProgress => "tool_progress",
            Self::ToolCompleted => "tool_completed",
            Self::ToolFailed => "tool_failed",
            Self::ArtifactCreated => "artifact_created",
            Self::SessionCompleted => "session_completed",
            Self::SessionCancelled => "session_cancelled",
            Self::SessionFailed => "session_failed",
        }
    }

    fn parse(value: &str) -> Result<Self, DigestError> {
        match value {
            "session_started" => Ok(Self::SessionStarted),
            "agent_message" => Ok(Self::AgentMessage),
            "agent_thinking" => Ok(Self::AgentThinking),
            "tool_started" => Ok(Self::ToolStarted),
            "tool_progress" => Ok(Self::ToolProgress),
            "tool_completed" => Ok(Self::ToolCompleted),
            "tool_failed" => Ok(Self::ToolFailed),
            "artifact_created" => Ok(Self::ArtifactCreated),
            "session_completed" => Ok(Self::SessionCompleted),
            "session_cancelled" => Ok(Self::SessionCancelled),
            "session_failed" => Ok(Self::SessionFailed),
            other => Err(DigestError::InvalidInput(format!(
                "unknown agent event kind: {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAgentEvent {
    pub job_id: String,
    pub session_id: String,
    pub kind: NewAgentEventKind,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub sequence: i64,
    pub job_id: String,
    pub session_id: String,
    pub kind: NewAgentEventKind,
    pub message: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRunAttempt {
    pub job_id: String,
    pub provider: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl AttemptStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Result<Self, DigestError> {
        match value {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(DigestError::InvalidInput(format!(
                "unknown attempt status: {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunAttempt {
    pub attempt_id: String,
    pub job_id: String,
    pub provider: String,
    pub provider_session_id: Option<String>,
    pub status: AttemptStatus,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Captured,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl From<AttemptStatus> for RunStatus {
    fn from(status: AttemptStatus) -> Self {
        match status {
            AttemptStatus::Running => Self::Running,
            AttemptStatus::Completed => Self::Completed,
            AttemptStatus::Failed => Self::Failed,
            AttemptStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub job_id: String,
    pub title: Option<String>,
    pub provider: Option<String>,
    pub status: RunStatus,
    pub updated_at_ms: i64,
    pub artifact_count: usize,
    pub event_count: usize,
}

#[derive(Clone, Debug)]
pub struct DigestService {
    data_dir: PathBuf,
    database_path: PathBuf,
    objects_dir: PathBuf,
}

impl DigestService {
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, DigestError> {
        let data_dir = data_dir.as_ref().to_path_buf();
        let objects_dir = data_dir.join("artifacts").join("objects");
        fs::create_dir_all(&objects_dir)?;
        let service = Self {
            database_path: data_dir.join("digest.db"),
            data_dir,
            objects_dir,
        };
        service.initialize_database()?;
        Ok(service)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn recover_abandoned_attempts(&self) -> Result<usize, DigestError> {
        let connection = self.connection()?;
        connection
            .execute(
                "UPDATE run_attempts
                 SET status = ?1, finished_at_ms = ?2, error = ?3
                 WHERE status = ?4",
                params![
                    AttemptStatus::Failed.as_str(),
                    now_ms(),
                    "Digest host was interrupted before the attempt completed",
                    AttemptStatus::Running.as_str(),
                ],
            )
            .map_err(Into::into)
    }

    pub fn write_analysis(&self, draft: AnalysisDraft) -> Result<ArtifactEnvelope, DigestError> {
        require_non_empty("jobId", &draft.job_id)?;
        require_non_empty("articleId", &draft.article_id)?;
        require_non_empty("summary", &draft.summary)?;

        let payload = serde_json::json!({
            "articleId": draft.article_id,
            "summary": draft.summary,
        });
        self.persist_json_artifact(&draft.job_id, ArtifactKind::Analysis, payload)
    }

    pub fn start_run_attempt(&self, input: StartRunAttempt) -> Result<RunAttempt, DigestError> {
        require_non_empty("jobId", &input.job_id)?;
        require_non_empty("provider", &input.provider)?;
        let started_at_ms = now_ms();
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO run_attempts (
                job_id, provider, status, started_at_ms
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                input.job_id,
                input.provider,
                AttemptStatus::Running.as_str(),
                started_at_ms
            ],
        )?;
        let row_id = connection.last_insert_rowid();
        let attempt_id = format!("attempt-{row_id}");
        connection.execute(
            "UPDATE run_attempts SET attempt_id = ?1 WHERE rowid = ?2",
            params![attempt_id, row_id],
        )?;
        self.find_run_attempt(&attempt_id)?.ok_or_else(|| {
            DigestError::InvalidInput(format!("attempt was not created: {attempt_id}"))
        })
    }

    pub fn finish_run_attempt(
        &self,
        attempt_id: &str,
        status: AttemptStatus,
        provider_session_id: Option<&str>,
        error: Option<&str>,
    ) -> Result<RunAttempt, DigestError> {
        require_non_empty("attemptId", attempt_id)?;
        if status == AttemptStatus::Running {
            return Err(DigestError::InvalidInput(
                "finished attempt cannot remain running".into(),
            ));
        }
        let connection = self.connection()?;
        let updated = connection.execute(
            "UPDATE run_attempts
             SET status = ?1, provider_session_id = ?2, finished_at_ms = ?3, error = ?4
             WHERE attempt_id = ?5",
            params![
                status.as_str(),
                provider_session_id,
                now_ms(),
                error,
                attempt_id
            ],
        )?;
        if updated == 0 {
            return Err(DigestError::InvalidInput(format!(
                "attempt not found: {attempt_id}"
            )));
        }
        self.find_run_attempt(attempt_id)?
            .ok_or_else(|| DigestError::InvalidInput(format!("attempt not found: {attempt_id}")))
    }

    pub fn list_run_attempts(&self, job_id: &str) -> Result<Vec<RunAttempt>, DigestError> {
        require_non_empty("jobId", job_id)?;
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT attempt_id, job_id, provider, provider_session_id, status,
                    started_at_ms, finished_at_ms, error
             FROM run_attempts WHERE job_id = ?1 ORDER BY rowid",
        )?;
        let rows = statement.query_map([job_id], run_attempt_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_runs(&self, limit: usize) -> Result<Vec<RunSummary>, DigestError> {
        let limit = limit.clamp(1, 100) as i64;
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "WITH activity AS (
                SELECT job_id, created_at_ms AS timestamp FROM artifacts
                UNION ALL
                SELECT job_id, created_at_ms AS timestamp FROM agent_events
                UNION ALL
                SELECT job_id, COALESCE(finished_at_ms, started_at_ms) AS timestamp
                FROM run_attempts
             ),
             jobs AS (
                SELECT job_id, MAX(timestamp) AS updated_at_ms
                FROM activity
                GROUP BY job_id
             )
             SELECT
                jobs.job_id,
                jobs.updated_at_ms,
                (SELECT provider FROM run_attempts
                 WHERE run_attempts.job_id = jobs.job_id
                 ORDER BY rowid DESC LIMIT 1),
                (SELECT status FROM run_attempts
                 WHERE run_attempts.job_id = jobs.job_id
                 ORDER BY rowid DESC LIMIT 1),
                (SELECT json_extract(payload_json, '$.title') FROM artifacts
                 WHERE artifacts.job_id = jobs.job_id
                   AND kind = 'normalized_article'
                 ORDER BY rowid DESC LIMIT 1),
                (SELECT COUNT(*) FROM artifacts
                 WHERE artifacts.job_id = jobs.job_id),
                (SELECT COUNT(*) FROM agent_events
                 WHERE agent_events.job_id = jobs.job_id)
             FROM jobs
             ORDER BY jobs.updated_at_ms DESC, jobs.job_id
             LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], |row| {
            let attempt_status: Option<String> = row.get(3)?;
            let status = attempt_status
                .map(|status| AttemptStatus::parse(&status).map(RunStatus::from))
                .transpose()
                .map_err(to_sql_error)?
                .unwrap_or(RunStatus::Captured);
            Ok(RunSummary {
                job_id: row.get(0)?,
                updated_at_ms: row.get(1)?,
                provider: row.get(2)?,
                status,
                title: row.get(4)?,
                artifact_count: row.get::<_, i64>(5)? as usize,
                event_count: row.get::<_, i64>(6)? as usize,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn read_artifact(&self, artifact_id: &str) -> Result<ArtifactEnvelope, DigestError> {
        require_non_empty("artifactId", artifact_id)?;
        self.find_artifact(artifact_id)?
            .ok_or_else(|| DigestError::ArtifactNotFound(artifact_id.into()))
    }

    pub fn list_artifacts(&self, job_id: &str) -> Result<Vec<ArtifactEnvelope>, DigestError> {
        require_non_empty("jobId", job_id)?;
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT artifact_id, schema_version, job_id, kind, content_hash, created_at_ms, payload_json
             FROM artifacts WHERE job_id = ?1 ORDER BY created_at_ms, artifact_id",
        )?;
        let rows = statement.query_map([job_id], artifact_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn latest_artifact(
        &self,
        job_id: &str,
        kind: ArtifactKind,
    ) -> Result<Option<ArtifactEnvelope>, DigestError> {
        require_non_empty("jobId", job_id)?;
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT artifact_id, schema_version, job_id, kind, content_hash, created_at_ms,
                        payload_json
                 FROM artifacts
                 WHERE job_id = ?1 AND kind = ?2
                 ORDER BY rowid DESC
                 LIMIT 1",
                params![job_id, kind.as_str()],
                artifact_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn record_agent_event(&self, event: NewAgentEvent) -> Result<AgentEvent, DigestError> {
        require_non_empty("jobId", &event.job_id)?;
        require_non_empty("sessionId", &event.session_id)?;
        require_non_empty("message", &event.message)?;
        let created_at_ms = now_ms();
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO agent_events (job_id, session_id, kind, message, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.job_id,
                event.session_id,
                event.kind.as_str(),
                event.message,
                created_at_ms
            ],
        )?;
        Ok(AgentEvent {
            sequence: connection.last_insert_rowid(),
            job_id: event.job_id,
            session_id: event.session_id,
            kind: event.kind,
            message: event.message,
            created_at_ms,
        })
    }

    pub fn list_agent_events(&self, job_id: &str) -> Result<Vec<AgentEvent>, DigestError> {
        require_non_empty("jobId", job_id)?;
        self.list_agent_events_after(job_id, None)
    }

    fn list_agent_events_after(
        &self,
        job_id: &str,
        after_sequence: Option<i64>,
    ) -> Result<Vec<AgentEvent>, DigestError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT sequence, job_id, session_id, kind, message, created_at_ms
             FROM agent_events
             WHERE job_id = ?1 AND (?2 IS NULL OR sequence > ?2)
             ORDER BY sequence",
        )?;
        let rows = statement.query_map(params![job_id, after_sequence], |row| {
            let kind: String = row.get(3)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                kind,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;

        rows.map(|row| {
            let (sequence, job_id, session_id, kind, message, created_at_ms) = row?;
            Ok(AgentEvent {
                sequence,
                job_id,
                session_id,
                kind: NewAgentEventKind::parse(&kind)?,
                message,
                created_at_ms,
            })
        })
        .collect()
    }

    pub fn list_presentation_events(&self, job_id: &str) -> Result<Vec<AgentEvent>, DigestError> {
        self.project_presentation_events(self.list_agent_events(job_id)?)
    }

    pub fn list_presentation_events_after(
        &self,
        job_id: &str,
        after_sequence: i64,
    ) -> Result<Vec<AgentEvent>, DigestError> {
        require_non_empty("jobId", job_id)?;
        self.project_presentation_events(
            self.list_agent_events_after(job_id, Some(after_sequence))?,
        )
    }

    fn project_presentation_events(
        &self,
        raw_events: Vec<AgentEvent>,
    ) -> Result<Vec<AgentEvent>, DigestError> {
        let mut events: Vec<AgentEvent> = Vec::with_capacity(raw_events.len());
        for mut event in raw_events {
            let is_streamed_text = matches!(
                event.kind,
                NewAgentEventKind::AgentMessage | NewAgentEventKind::AgentThinking
            );
            let Some(text) = is_streamed_text
                .then(|| streamed_text(&event.message))
                .flatten()
            else {
                events.push(event);
                continue;
            };
            if let Some(previous) = events.last_mut().filter(|previous| {
                previous.session_id == event.session_id && previous.kind == event.kind
            }) {
                previous.message.push_str(&text);
                previous.sequence = event.sequence;
                previous.created_at_ms = event.created_at_ms;
                continue;
            }
            event.message = text;
            events.push(event);
        }
        Ok(events)
    }

    fn initialize_database(&self) -> Result<(), DigestError> {
        let connection = self.connection()?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS artifacts (
                artifact_id TEXT PRIMARY KEY,
                schema_version TEXT NOT NULL,
                job_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                payload_json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS artifacts_job_id ON artifacts(job_id);
             CREATE TABLE IF NOT EXISTS agent_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                message TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS agent_events_job_id ON agent_events(job_id, sequence);",
        )?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS run_attempts (
                attempt_id TEXT UNIQUE,
                job_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                provider_session_id TEXT,
                status TEXT NOT NULL,
                started_at_ms INTEGER NOT NULL,
                finished_at_ms INTEGER,
                error TEXT
             );
             CREATE INDEX IF NOT EXISTS run_attempts_job_id
             ON run_attempts(job_id, started_at_ms);",
        )?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection, DigestError> {
        Ok(Connection::open(&self.database_path)?)
    }

    fn find_artifact(&self, artifact_id: &str) -> Result<Option<ArtifactEnvelope>, DigestError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT artifact_id, schema_version, job_id, kind, content_hash, created_at_ms, payload_json
                 FROM artifacts WHERE artifact_id = ?1",
                [artifact_id],
                artifact_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    fn find_run_attempt(&self, attempt_id: &str) -> Result<Option<RunAttempt>, DigestError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT attempt_id, job_id, provider, provider_session_id, status,
                        started_at_ms, finished_at_ms, error
                 FROM run_attempts WHERE attempt_id = ?1",
                [attempt_id],
                run_attempt_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub(crate) fn persist_json_artifact(
        &self,
        job_id: &str,
        kind: ArtifactKind,
        payload: Value,
    ) -> Result<ArtifactEnvelope, DigestError> {
        let bytes = serde_json::to_vec(&payload)?;
        let content_hash = sha256_hex(&bytes);
        self.write_object(&content_hash, "json", &bytes)?;
        self.persist_artifact(job_id, kind, content_hash.clone(), content_hash, payload)
    }

    pub(crate) fn persist_binary_artifact(
        &self,
        job_id: &str,
        kind: ArtifactKind,
        bytes: &[u8],
        payload: Value,
    ) -> Result<ArtifactEnvelope, DigestError> {
        let content_hash = sha256_hex(bytes);
        self.write_object(&content_hash, "bin", bytes)?;
        let metadata_hash = sha256_hex(&serde_json::to_vec(&payload)?);
        let identity_hash = sha256_hex(format!("{content_hash}\0{metadata_hash}").as_bytes());
        self.persist_artifact(job_id, kind, content_hash, identity_hash, payload)
    }

    fn persist_artifact(
        &self,
        job_id: &str,
        kind: ArtifactKind,
        content_hash: String,
        identity_hash: String,
        payload: Value,
    ) -> Result<ArtifactEnvelope, DigestError> {
        require_non_empty("jobId", job_id)?;
        let artifact_id =
            sha256_hex(format!("{}\0{}\0{}", kind.as_str(), job_id, identity_hash).as_bytes());
        if let Some(existing) = self.find_artifact(&artifact_id)? {
            return Ok(existing);
        }
        let artifact = ArtifactEnvelope {
            schema_version: ARTIFACT_SCHEMA_VERSION.into(),
            artifact_id,
            job_id: job_id.into(),
            kind,
            content_hash,
            created_at_ms: now_ms(),
            payload,
        };
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR IGNORE INTO artifacts (
                artifact_id, schema_version, job_id, kind, content_hash, created_at_ms, payload_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                artifact.artifact_id,
                artifact.schema_version,
                artifact.job_id,
                artifact.kind.as_str(),
                artifact.content_hash,
                artifact.created_at_ms,
                serde_json::to_string(&artifact.payload)?,
            ],
        )?;
        self.find_artifact(&artifact.artifact_id)?
            .ok_or_else(|| DigestError::ArtifactNotFound(artifact.artifact_id))
    }

    fn write_object(
        &self,
        content_hash: &str,
        extension: &str,
        bytes: &[u8],
    ) -> Result<(), DigestError> {
        let destination = self.objects_dir.join(format!("{content_hash}.{extension}"));
        if destination.exists() {
            return Ok(());
        }

        let temporary = self.objects_dir.join(format!(
            ".{content_hash}.{}.{}.tmp",
            std::process::id(),
            TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let write_result = (|| -> Result<(), std::io::Error> {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &destination)
        })();
        match write_result {
            Ok(()) => Ok(()),
            Err(_error) if destination.exists() => {
                let _ = fs::remove_file(temporary);
                Ok(())
            }
            Err(error) => {
                let _ = fs::remove_file(temporary);
                Err(error.into())
            }
        }
    }
}

fn artifact_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactEnvelope> {
    let kind: String = row.get(3)?;
    let payload_json: String = row.get(6)?;
    let artifact_kind = ArtifactKind::parse(&kind).map_err(to_sql_error)?;
    let payload = serde_json::from_str(&payload_json).map_err(to_sql_error)?;
    Ok(ArtifactEnvelope {
        artifact_id: row.get(0)?,
        schema_version: row.get(1)?,
        job_id: row.get(2)?,
        kind: artifact_kind,
        content_hash: row.get(4)?,
        created_at_ms: row.get(5)?,
        payload,
    })
}

fn run_attempt_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunAttempt> {
    let status: String = row.get(4)?;
    Ok(RunAttempt {
        attempt_id: row.get(0)?,
        job_id: row.get(1)?,
        provider: row.get(2)?,
        provider_session_id: row.get(3)?,
        status: AttemptStatus::parse(&status).map_err(to_sql_error)?,
        started_at_ms: row.get(5)?,
        finished_at_ms: row.get(6)?,
        error: row.get(7)?,
    })
}

fn to_sql_error(error: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn require_non_empty(name: &str, value: &str) -> Result<(), DigestError> {
    if value.trim().is_empty() {
        return Err(DigestError::InvalidInput(format!("{name} is required")));
    }
    Ok(())
}

fn streamed_text(message: &str) -> Option<String> {
    serde_json::from_str::<Value>(message)
        .ok()?
        .pointer("/content/text")?
        .as_str()
        .map(str::to_owned)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
