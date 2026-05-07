use std::fs;
use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, ClassificationInput, ClassifierLogger, RedKind,
    Verdict,
};
use file_logger::FileLogger;
use tempfile::tempdir;

fn input() -> ClassificationInput {
    ClassificationInput {
        agent: Agent::root(AgentId::try_new("sess").unwrap()),
        action: Action {
            call_id: None,
            kind: ActionKind::new("Bash"),
            args: ActionArgs::new("ls"),
        },
        working_dir: PathBuf::from("/tmp"),
    }
}

#[test]
fn log_incoming_writes_one_line() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileLogger::new(log.clone(), Vec::new());

    logger.log_incoming(&input());

    let content = fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("incoming: "));
    assert!(lines[0].contains("id=sess"));
    assert!(lines[0].contains("kind=Bash"));
}

#[test]
fn log_outcome_green() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileLogger::new(log.clone(), Vec::new());

    logger.log_outcome(&input(), &Verdict::Green);

    let content = fs::read_to_string(&log).unwrap();
    assert_eq!(content.trim(), "outcome: id=sess verdict=Green");
}

#[test]
fn log_outcome_red_includes_kind_and_reason() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("remit.log");
    let logger = FileLogger::new(log.clone(), Vec::new());

    logger.log_outcome(
        &input(),
        &Verdict::Red {
            kind: RedKind::ScopeSetupCancelled,
            reason: "scope setup cancelled; nothing was set".into(),
        },
    );

    let content = fs::read_to_string(&log).unwrap();
    assert!(content.contains("verdict=Red"));
    assert!(content.contains("kind=ScopeSetupCancelled"));
    assert!(content.contains("scope setup cancelled"));
}

#[test]
fn log_error_walks_source_chain() {
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
    let logger = FileLogger::new(log.clone(), Vec::new());

    let err = Outer(Inner("disk full"));
    logger.log_error(&input(), &err);

    let content = fs::read_to_string(&log).unwrap();
    assert!(content.starts_with("error: id=sess "));
    assert!(content.contains("outer wrapper"));
    assert!(content.contains("inner cause"));
    assert!(content.contains("disk full"));
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
    let logger = Arc::new(FileLogger::new(log.clone(), Vec::new()));

    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let logger = Arc::clone(&logger);
            thread::spawn(move || {
                let marker = format!("THREAD-{t:03}");
                let body = "x".repeat(BODY_SIZE);
                for i in 0..LINES_PER_THREAD {
                    logger.log_line(&format!("{marker} seq={i:04} body={body}"));
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

    for (idx, line) in lines.iter().enumerate() {
        let first_marker = &line[..line.find(' ').unwrap_or(line.len())];
        assert!(
            first_marker.starts_with("THREAD-"),
            "line {idx} does not start with a marker: {line:?}"
        );
        let marker_count = line.matches("THREAD-").count();
        assert_eq!(
            marker_count, 1,
            "line {idx} contains {marker_count} markers (expected 1) — interleaved write: {line:?}",
        );
        assert!(
            line.contains(" seq="),
            "line {idx} missing seq field: {line:?}"
        );
        let body_len = line.matches('x').count();
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
    let logger = FileLogger::new(log.clone(), Vec::new());

    logger.log_incoming(&input());
    logger.log_outcome(&input(), &Verdict::Green);

    let content = fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("incoming: "));
    assert!(lines[1].starts_with("outcome: "));
}
