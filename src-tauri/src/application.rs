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
}

impl ArtifactKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
        }
    }

    fn parse(value: &str) -> Result<Self, DigestError> {
        match value {
            "analysis" => Ok(Self::Analysis),
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
    ArtifactCreated,
    SessionCompleted,
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
            Self::ArtifactCreated => "artifact_created",
            Self::SessionCompleted => "session_completed",
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
            "artifact_created" => Ok(Self::ArtifactCreated),
            "session_completed" => Ok(Self::SessionCompleted),
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

    pub fn write_analysis(&self, draft: AnalysisDraft) -> Result<ArtifactEnvelope, DigestError> {
        require_non_empty("jobId", &draft.job_id)?;
        require_non_empty("articleId", &draft.article_id)?;
        require_non_empty("summary", &draft.summary)?;

        let payload = serde_json::json!({
            "articleId": draft.article_id,
            "summary": draft.summary,
        });
        let payload_bytes = serde_json::to_vec(&payload)?;
        let content_hash = sha256_hex(&payload_bytes);
        let artifact_id = sha256_hex(
            format!(
                "{}\0{}\0{}",
                ArtifactKind::Analysis.as_str(),
                draft.job_id,
                content_hash
            )
            .as_bytes(),
        );

        if let Some(existing) = self.find_artifact(&artifact_id)? {
            return Ok(existing);
        }

        self.write_object(&content_hash, &payload_bytes)?;
        let artifact = ArtifactEnvelope {
            schema_version: ARTIFACT_SCHEMA_VERSION.into(),
            artifact_id,
            job_id: draft.job_id,
            kind: ArtifactKind::Analysis,
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
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT sequence, job_id, session_id, kind, message, created_at_ms
             FROM agent_events WHERE job_id = ?1 ORDER BY sequence",
        )?;
        let rows = statement.query_map([job_id], |row| {
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

    fn write_object(&self, content_hash: &str, bytes: &[u8]) -> Result<(), DigestError> {
        let destination = self.objects_dir.join(format!("{content_hash}.json"));
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

fn to_sql_error(error: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn require_non_empty(name: &str, value: &str) -> Result<(), DigestError> {
    if value.trim().is_empty() {
        return Err(DigestError::InvalidInput(format!("{name} is required")));
    }
    Ok(())
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
