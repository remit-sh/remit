use std::fs;
use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, ClassificationInput, ClassifierLogger,
};
use tempfile::tempdir;

use super::*;

fn read_log(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap()
}

/// Strip the leading RFC 3339 UTC timestamp + space from a log line, after
/// asserting the prefix is well-formed.
fn strip_timestamp(line: &str) -> &str {
    let (timestamp, rest) = line
        .split_once(' ')
        .unwrap_or_else(|| panic!("log line has no timestamp/body separator: {line:?}"));
    assert_timestamp_format(timestamp);
    rest
}

fn assert_timestamp_format(timestamp: &str) {
    // Format: YYYY-MM-DDTHH:MM:SS.mmmZ — fixed width, 24 chars
    assert_eq!(
        timestamp.len(),
        24,
        "expected 24-char RFC 3339 ms-precision timestamp, got {timestamp:?}"
    );
    let bytes = timestamp.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        let ok = match i {
            4 | 7 => *b == b'-',
            10 => *b == b'T',
            13 | 16 => *b == b':',
            19 => *b == b'.',
            23 => *b == b'Z',
            _ => b.is_ascii_digit(),
        };
        assert!(ok, "timestamp char at index {i} unexpected: {timestamp:?}");
    }
}

#[test]
fn log_line_prefixes_timestamp() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileLogger::new(path.clone(), vec![]);

    logger.log_line("hello");

    let contents = read_log(&path);
    let line = contents.strip_suffix('\n').unwrap();
    assert_eq!(strip_timestamp(line), "hello");
}

#[test]
fn log_line_writes_line_unchanged_with_no_markers() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileLogger::new(path.clone(), vec![]);

    logger.log_line("hello");

    let contents = read_log(&path);
    let line = contents.strip_suffix('\n').unwrap();
    assert_eq!(strip_timestamp(line), "hello");
}

#[test]
fn log_line_prefixes_single_marker() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileLogger::new(path.clone(), vec!["[dev-bypass-active]".into()]);

    logger.log_line("hello");

    let contents = read_log(&path);
    let line = contents.strip_suffix('\n').unwrap();
    assert_eq!(strip_timestamp(line), "[dev-bypass-active] hello");
}

#[test]
fn log_line_space_joins_multiple_markers() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileLogger::new(path.clone(), vec!["[a]".into(), "[b]".into()]);

    logger.log_line("hello");

    let contents = read_log(&path);
    let line = contents.strip_suffix('\n').unwrap();
    assert_eq!(strip_timestamp(line), "[a] [b] hello");
}

#[test]
fn log_line_prefixes_every_line() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileLogger::new(path.clone(), vec!["[mark]".into()]);

    logger.log_line("first");
    logger.log_line("second");

    let contents = read_log(&path);
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(strip_timestamp(lines[0]), "[mark] first");
    assert_eq!(strip_timestamp(lines[1]), "[mark] second");
}

#[test]
fn log_incoming_carries_marker() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileLogger::new(path.clone(), vec!["[dev-bypass-active]".into()]);

    let input = ClassificationInput {
        agent: Agent::root(AgentId::try_new("session-1").unwrap()),
        action: Action {
            call_id: None,
            kind: ActionKind::new("Bash"),
            args: ActionArgs::new("{}"),
        },
        working_dir: PathBuf::from("/tmp"),
    };

    logger.log_incoming(&input);

    let contents = read_log(&path);
    let line = contents.lines().next().unwrap();
    assert!(
        strip_timestamp(line).starts_with("[dev-bypass-active] incoming:"),
        "expected marker + incoming prefix after timestamp, got: {line:?}"
    );
}
