use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use domain::{
    ActionKind, AgentId, ClassificationEvent, ClassificationHistoryWriter, ClassificationRecord,
    CorrelationId, Enforced, EventKind, Verdict,
};
use file_classification_history::FileClassificationHistoryWriter;
use serde_json::Value;
use tempfile::tempdir;

fn record() -> ClassificationRecord {
    ClassificationRecord {
        ts: time::OffsetDateTime::now_utc(),
        session_id: AgentId::try_new("agent-1").unwrap(),
        parent_id: None,
        agent_type: None,
        working_dir: PathBuf::from("/tmp"),
        call_id: None,
        correlation_id: CorrelationId::new(),
        pid: 1,
        hook_duration: Duration::from_millis(1),
        tool_name: ActionKind::new("Bash"),
        tool_input: None,
        bundled_scope_present: false,
        narration_fields_stripped: 0,
        args_size_pre: 0,
        args_size_post: 0,
        scope: None,
        scope_origin: None,
        llm_outcome: None,
        prompt_shown: false,
        prompt_latency: None,
        lock_wait: None,
        initial_verdict: None,
        verdict: Verdict::Green,
        enforced: Enforced::Permit,
        override_action: None,
        error_chain: None,
        dev_bypass: false,
        details: BTreeMap::new(),
    }
}

#[test]
fn appends_across_multiple_calls() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(log.clone());

    writer.write(record()).unwrap();
    writer.write(record()).unwrap();

    let content = fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    for line in &lines {
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["event_kind"], "classification");
    }
}

#[test]
fn event_kind_distinguishes_records_and_events() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(log.clone());

    writer.write(record()).unwrap();
    writer
        .write_event(ClassificationEvent {
            ts: time::OffsetDateTime::now_utc(),
            session_id: None,
            correlation_id: None,
            kind: EventKind::Lifecycle,
            message: "hook-event: PreToolUse".into(),
            details: BTreeMap::new(),
            dev_bypass: false,
        })
        .unwrap();
    writer
        .write_event(ClassificationEvent {
            ts: time::OffsetDateTime::now_utc(),
            session_id: Some(AgentId::try_new("agent-1").unwrap()),
            correlation_id: Some(CorrelationId::new()),
            kind: EventKind::StateTransition,
            message: "user-allow".into(),
            details: BTreeMap::new(),
            dev_bypass: false,
        })
        .unwrap();

    let content = fs::read_to_string(&log).unwrap();
    let kinds: Vec<String> = content
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["event_kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["classification", "lifecycle", "state-transition"]
    );
}

#[test]
fn concurrent_threads_write_whole_lines() {
    const THREADS: usize = 16;
    const LINES_PER_THREAD: usize = 50;

    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let writer = Arc::new(FileClassificationHistoryWriter::new(log.clone()));

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let writer = Arc::clone(&writer);
            thread::spawn(move || {
                for _ in 0..LINES_PER_THREAD {
                    writer.write(record()).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    let content = fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), THREADS * LINES_PER_THREAD);
    for (idx, line) in lines.iter().enumerate() {
        let v: Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("line {idx} not valid JSON: {e}: {line:?}"));
        assert_eq!(v["event_kind"], "classification");
    }
}
