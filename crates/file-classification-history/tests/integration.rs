use std::fs;
use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, ClassificationHistoryWriter,
    ClassificationInput, RedKind, Verdict,
};
use file_classification_history::FileClassificationHistoryWriter;
use serde_json::Value;
use tempfile::tempdir;

fn input() -> ClassificationInput {
    ClassificationInput {
        agent: Agent::root(AgentId::try_new("agent-1").unwrap()),
        action: Action {
            call_id: None,
            kind: ActionKind::new("Bash"),
            args: ActionArgs::new("{\"command\":\"ls\"}"),
        },
        working_dir: PathBuf::from("/tmp"),
    }
}

fn parse_lines(contents: &str) -> Vec<Value> {
    contents
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str::<Value>(l).expect("ndjson line is JSON"))
        .collect()
}

#[test]
fn record_incoming_writes_one_ndjson_record() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(log.clone(), Vec::new(), false);

    logger.record_incoming(&input());

    let lines = parse_lines(&fs::read_to_string(&log).unwrap());
    assert_eq!(lines.len(), 1);
    let rec = &lines[0];
    assert_eq!(rec["event"], "incoming");
    assert_eq!(rec["agent_id"], "agent-1");
    assert_eq!(rec["action"], "Bash");
}

#[test]
fn record_outcome_green_emits_verdict_field() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(log.clone(), Vec::new(), false);

    logger.record_outcome(&input(), &Verdict::Green);

    let rec = &parse_lines(&fs::read_to_string(&log).unwrap())[0];
    assert_eq!(rec["event"], "outcome");
    assert_eq!(rec["verdict"], "green");
    assert_eq!(rec["agent_id"], "agent-1");
    assert_eq!(rec["action"], "Bash");
    assert!(rec.get("red_kind").is_none());
    assert!(rec.get("reason").is_none());
}

#[test]
fn record_outcome_red_includes_kind_and_reason() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(log.clone(), Vec::new(), false);

    logger.record_outcome(
        &input(),
        &Verdict::Red {
            kind: RedKind::ScopeSetupCancelled,
            reason: "scope setup cancelled; nothing was set".into(),
        },
    );

    let rec = &parse_lines(&fs::read_to_string(&log).unwrap())[0];
    assert_eq!(rec["event"], "outcome");
    assert_eq!(rec["verdict"], "red");
    assert_eq!(rec["red_kind"], "ScopeSetupCancelled");
    assert_eq!(rec["reason"], "scope setup cancelled; nothing was set");
}

#[test]
fn record_outcome_omits_args_by_default() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(log.clone(), Vec::new(), false);

    logger.record_outcome(&input(), &Verdict::Green);

    let rec = &parse_lines(&fs::read_to_string(&log).unwrap())[0];
    assert!(rec.get("args").is_none());
}

#[test]
fn record_outcome_carries_args_when_debug_mode() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(log.clone(), Vec::new(), true);

    logger.record_outcome(&input(), &Verdict::Green);

    let rec = &parse_lines(&fs::read_to_string(&log).unwrap())[0];
    assert_eq!(rec["args"], "{\"command\":\"ls\"}");
}

#[test]
fn record_error_walks_source_chain() {
    use std::error::Error;
    use std::fmt;

    #[derive(Debug)]
    struct Outer(Inner);
    #[derive(Debug)]
    struct Inner(&'static str);

    impl fmt::Display for Outer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "outer wrapper")
        }
    }
    impl fmt::Display for Inner {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "inner cause: {}", self.0)
        }
    }
    impl Error for Outer {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }
    impl Error for Inner {}

    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(log.clone(), Vec::new(), false);

    let err = Outer(Inner("disk full"));
    logger.record_error(&input(), &err);

    let rec = &parse_lines(&fs::read_to_string(&log).unwrap())[0];
    assert_eq!(rec["event"], "error");
    assert_eq!(rec["agent_id"], "agent-1");
    let chain = rec["chain"].as_str().unwrap();
    assert!(chain.contains("outer wrapper"));
    assert!(chain.contains("inner cause"));
    assert!(chain.contains("disk full"));
}

#[test]
fn timestamps_are_rfc3339_utc() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(log.clone(), Vec::new(), false);

    logger.record_incoming(&input());

    let rec = &parse_lines(&fs::read_to_string(&log).unwrap())[0];
    let ts = rec["ts"].as_str().unwrap();
    // Rfc3339 UTC ends in 'Z' and contains 'T' separator. Cheap shape check
    // that doesn't pin the subsecond precision.
    assert!(ts.contains('T'), "ts has date+time separator: {ts}");
    assert!(ts.ends_with('Z'), "ts is UTC (zulu): {ts}");
}

#[test]
fn markers_appear_as_field_not_prefix() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(
        log.clone(),
        vec!["dev-bypass-active".into(), "debug-mode".into()],
        false,
    );

    logger.record_incoming(&input());

    let raw = fs::read_to_string(&log).unwrap();
    // Records are pure JSON — no free-text prefix.
    assert!(raw.starts_with('{'), "ndjson starts with '{{': {raw:?}");
    let rec = &parse_lines(&raw)[0];
    assert_eq!(
        rec["markers"],
        serde_json::json!(["dev-bypass-active", "debug-mode"])
    );
}

#[test]
fn concurrent_threads_write_whole_lines() {
    use std::sync::Arc;
    use std::thread;

    const THREADS: usize = 16;
    const LINES_PER_THREAD: usize = 50;
    const BODY_SIZE: usize = 8 * 1024;

    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = Arc::new(FileClassificationHistoryWriter::new(
        log.clone(),
        Vec::new(),
        false,
    ));

    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let logger = Arc::clone(&logger);
            thread::spawn(move || {
                let marker = format!("THREAD-{t:03}");
                let body = "x".repeat(BODY_SIZE);
                for i in 0..LINES_PER_THREAD {
                    logger.record_hook_event(&format!("{marker} seq={i:04} body={body}"));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    let content = fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    assert_eq!(
        lines.len(),
        THREADS * LINES_PER_THREAD,
        "expected {} lines, got {}",
        THREADS * LINES_PER_THREAD,
        lines.len()
    );

    for (idx, raw) in lines.iter().enumerate() {
        let rec: Value =
            serde_json::from_str(raw).expect("line {idx} is whole NDJSON, not interleaved");
        let name = rec["name"].as_str().expect("name field present");
        let marker_count = name.matches("THREAD-").count();
        assert_eq!(
            marker_count, 1,
            "line {idx} name contains {marker_count} markers (expected 1) — interleaved write: {raw:?}",
        );
        assert!(
            name.contains(" seq="),
            "line {idx} missing seq field: {raw:?}"
        );
        let body_len = name.matches('x').count();
        assert_eq!(
            body_len, BODY_SIZE,
            "line {idx} body length {body_len} != {BODY_SIZE}: truncated or fused",
        );
    }
}

#[test]
fn appends_across_multiple_calls() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(log.clone(), Vec::new(), false);

    logger.record_incoming(&input());
    logger.record_outcome(&input(), &Verdict::Green);

    let lines = parse_lines(&fs::read_to_string(&log).unwrap());
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["event"], "incoming");
    assert_eq!(lines[1]["event"], "outcome");
}
