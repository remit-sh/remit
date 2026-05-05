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

#[test]
fn log_line_writes_line_unchanged_with_no_markers() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileLogger::new(path.clone(), vec![]);

    logger.log_line("hello");

    assert_eq!(read_log(&path), "hello\n");
}

#[test]
fn log_line_prefixes_single_marker() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileLogger::new(path.clone(), vec!["[dev-bypass-active]".into()]);

    logger.log_line("hello");

    assert_eq!(read_log(&path), "[dev-bypass-active] hello\n");
}

#[test]
fn log_line_space_joins_multiple_markers() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileLogger::new(path.clone(), vec!["[a]".into(), "[b]".into()]);

    logger.log_line("hello");

    assert_eq!(read_log(&path), "[a] [b] hello\n");
}

#[test]
fn log_line_prefixes_every_line() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileLogger::new(path.clone(), vec!["[mark]".into()]);

    logger.log_line("first");
    logger.log_line("second");

    assert_eq!(read_log(&path), "[mark] first\n[mark] second\n");
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
    assert!(
        contents.starts_with("[dev-bypass-active] incoming:"),
        "expected prefix, got: {contents:?}"
    );
}
