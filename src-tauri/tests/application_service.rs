use digest_lib::{
    AnalysisDraft, ArtifactKind, AttemptStatus, DigestService, NewAgentEvent, NewAgentEventKind,
    RunStatus, StartRunAttempt,
};

#[test]
fn analysis_round_trips_through_the_application_service() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = DigestService::open(directory.path()).expect("open Digest service");

    let created = service
        .write_analysis(AnalysisDraft {
            job_id: "job-1".into(),
            article_id: "article-1".into(),
            summary: "The article explains a replicated log.".into(),
        })
        .expect("write analysis");

    let loaded = service
        .read_artifact(&created.artifact_id)
        .expect("read artifact");

    assert_eq!(created, loaded);
    assert_eq!(loaded.kind, ArtifactKind::Analysis);
    assert_eq!(loaded.job_id, "job-1");
    assert_eq!(
        loaded.payload,
        serde_json::json!({
            "articleId": "article-1",
            "summary": "The article explains a replicated log."
        })
    );
    assert_eq!(loaded.content_hash.len(), 64);
}

#[test]
fn identical_analysis_is_content_addressed_and_idempotent() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = DigestService::open(directory.path()).expect("open Digest service");
    let draft = AnalysisDraft {
        job_id: "job-1".into(),
        article_id: "article-1".into(),
        summary: "A stable summary.".into(),
    };

    let first = service
        .write_analysis(draft.clone())
        .expect("write first analysis");
    let second = service.write_analysis(draft).expect("write same analysis");

    assert_eq!(first.artifact_id, second.artifact_id);
    assert_eq!(service.list_artifacts("job-1").unwrap(), vec![first]);
}

#[test]
fn latest_artifact_returns_the_last_artifact_of_the_requested_kind() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = DigestService::open(directory.path()).expect("open Digest service");
    let first = service
        .write_analysis(AnalysisDraft {
            job_id: "job-1".into(),
            article_id: "article-1".into(),
            summary: "First analysis.".into(),
        })
        .expect("persist first analysis");
    let second = service
        .write_analysis(AnalysisDraft {
            job_id: "job-1".into(),
            article_id: "article-1".into(),
            summary: "Second analysis.".into(),
        })
        .expect("persist second analysis");

    assert_ne!(first.artifact_id, second.artifact_id);
    assert_eq!(
        service
            .latest_artifact("job-1", ArtifactKind::Analysis)
            .expect("query latest")
            .expect("latest analysis")
            .artifact_id,
        second.artifact_id
    );
    assert!(service
        .latest_artifact("job-1", ArtifactKind::NarrationPlan)
        .expect("query absent kind")
        .is_none());
}

#[test]
fn canonical_agent_events_are_ordered_and_job_scoped() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = DigestService::open(directory.path()).expect("open Digest service");

    service
        .record_agent_event(NewAgentEvent {
            job_id: "job-1".into(),
            session_id: "session-1".into(),
            kind: NewAgentEventKind::SessionStarted,
            message: "OpenCode session started".into(),
        })
        .expect("record first event");
    service
        .record_agent_event(NewAgentEvent {
            job_id: "job-1".into(),
            session_id: "session-1".into(),
            kind: NewAgentEventKind::ArtifactCreated,
            message: "Analysis artifact created".into(),
        })
        .expect("record second event");
    service
        .record_agent_event(NewAgentEvent {
            job_id: "job-1".into(),
            session_id: "session-1".into(),
            kind: NewAgentEventKind::ToolFailed,
            message: "Invalid tool arguments".into(),
        })
        .expect("record failed tool event");
    service
        .record_agent_event(NewAgentEvent {
            job_id: "job-2".into(),
            session_id: "session-2".into(),
            kind: NewAgentEventKind::SessionStarted,
            message: "Other session".into(),
        })
        .expect("record unrelated event");

    let events = service.list_agent_events("job-1").expect("list events");

    assert_eq!(events.len(), 3);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[1].sequence, 2);
    assert_eq!(events[1].kind, NewAgentEventKind::ArtifactCreated);
    assert_eq!(events[2].kind, NewAgentEventKind::ToolFailed);
}

#[test]
fn run_attempts_preserve_retries_as_separate_outcomes() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = DigestService::open(directory.path()).expect("open Digest service");

    let failed = service
        .start_run_attempt(StartRunAttempt {
            job_id: "job-1".into(),
            provider: "open_code".into(),
        })
        .expect("start failed attempt");
    service
        .finish_run_attempt(
            &failed.attempt_id,
            AttemptStatus::Failed,
            Some("session-failed"),
            Some("provider unavailable"),
        )
        .expect("finish failed attempt");
    let completed = service
        .start_run_attempt(StartRunAttempt {
            job_id: "job-1".into(),
            provider: "open_code".into(),
        })
        .expect("start completed attempt");
    service
        .finish_run_attempt(
            &completed.attempt_id,
            AttemptStatus::Completed,
            Some("session-completed"),
            None,
        )
        .expect("finish completed attempt");

    let attempts = service.list_run_attempts("job-1").expect("list attempts");

    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].status, AttemptStatus::Failed);
    assert_eq!(
        attempts[0].provider_session_id.as_deref(),
        Some("session-failed")
    );
    assert_eq!(attempts[1].status, AttemptStatus::Completed);
    assert_ne!(attempts[0].attempt_id, attempts[1].attempt_id);
}

#[test]
fn presentation_events_coalesce_streamed_text_chunks() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = DigestService::open(directory.path()).expect("open Digest service");
    for text in ["Hello", " world"] {
        service
            .record_agent_event(NewAgentEvent {
                job_id: "job-1".into(),
                session_id: "session-1".into(),
                kind: NewAgentEventKind::AgentMessage,
                message: serde_json::json!({
                    "sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": text}
                })
                .to_string(),
            })
            .expect("record message chunk");
    }
    service
        .record_agent_event(NewAgentEvent {
            job_id: "job-1".into(),
            session_id: "session-1".into(),
            kind: NewAgentEventKind::SessionCompleted,
            message: "done".into(),
        })
        .expect("record completion");

    let events = service
        .list_presentation_events("job-1")
        .expect("list presentation events");

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, NewAgentEventKind::AgentMessage);
    assert_eq!(events[0].message, "Hello world");
}

#[test]
fn reopening_the_service_marks_abandoned_attempts_as_failed() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = DigestService::open(directory.path()).expect("open Digest service");
    service
        .start_run_attempt(StartRunAttempt {
            job_id: "job-interrupted".into(),
            provider: "open_code".into(),
        })
        .expect("start attempt");
    drop(service);

    let reopened = DigestService::open(directory.path()).expect("reopen Digest service");
    reopened
        .recover_abandoned_attempts()
        .expect("recover attempts at host startup");
    let attempts = reopened
        .list_run_attempts("job-interrupted")
        .expect("list recovered attempts");

    assert_eq!(attempts[0].status, AttemptStatus::Failed);
    assert!(attempts[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("interrupted")));
    assert!(attempts[0].finished_at_ms.is_some());
}

#[test]
fn recent_runs_are_ordered_and_summarize_the_latest_attempt() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = DigestService::open(directory.path()).expect("open Digest service");
    service
        .write_analysis(AnalysisDraft {
            job_id: "job-older".into(),
            article_id: "article-1".into(),
            summary: "Older run".into(),
        })
        .expect("write older artifact");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let attempt = service
        .start_run_attempt(StartRunAttempt {
            job_id: "job-newer".into(),
            provider: "open_code".into(),
        })
        .expect("start newer run");
    service
        .finish_run_attempt(
            &attempt.attempt_id,
            AttemptStatus::Failed,
            Some("session-1"),
            Some("test failure"),
        )
        .expect("finish newer run");

    let runs = service.list_runs(10).expect("list recent runs");

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].job_id, "job-newer");
    assert_eq!(runs[0].provider.as_deref(), Some("open_code"));
    assert_eq!(runs[0].status, RunStatus::Failed);
    assert_eq!(runs[1].job_id, "job-older");
    assert_eq!(runs[1].status, RunStatus::Captured);
    assert_eq!(runs[1].artifact_count, 1);
}
