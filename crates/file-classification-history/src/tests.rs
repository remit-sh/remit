use std::fs;
use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, ClassificationHistoryWriter,
    ClassificationInput,
};
use serde_json::Value;
use tempfile::tempdir;

use super::*;

fn read_log(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap()
}

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
        .map(|l| serde_json::from_str::<Value>(l).expect("each line is JSON"))
        .collect()
}

#[test]
fn record_hook_event_emits_ndjson_with_name() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(path.clone(), vec![], false);

    logger.record_hook_event("PreToolUse");

    let rec = &parse_lines(&read_log(&path))[0];
    assert_eq!(rec["event"], "hook");
    assert_eq!(rec["name"], "PreToolUse");
}

#[test]
fn record_custom_merges_payload_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(path.clone(), vec![], false);

    logger.record_custom(
        "stage_pending",
        serde_json::json!({
            "parent": "parent-1",
            "outcome": "set",
        }),
    );

    let rec = &parse_lines(&read_log(&path))[0];
    assert_eq!(rec["event"], "stage_pending");
    assert_eq!(rec["parent"], "parent-1");
    assert_eq!(rec["outcome"], "set");
    assert!(rec["ts"].as_str().is_some_and(|s| !s.is_empty()));
}

#[test]
fn record_custom_carries_markers_when_set() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger =
        FileClassificationHistoryWriter::new(path.clone(), vec!["dev-bypass-active".into()], false);

    logger.record_custom("claim_pending", serde_json::json!({"outcome": "hit"}));

    let rec = &parse_lines(&read_log(&path))[0];
    assert_eq!(rec["markers"], serde_json::json!(["dev-bypass-active"]));
}

#[test]
fn record_custom_writes_event_only_when_payload_is_non_object() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(path.clone(), vec![], false);

    logger.record_custom("note", serde_json::Value::Null);

    let rec = &parse_lines(&read_log(&path))[0];
    assert_eq!(rec["event"], "note");
    assert!(rec["ts"].as_str().is_some_and(|s| !s.is_empty()));
}

#[test]
fn record_incoming_emits_ndjson_with_required_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(path.clone(), vec![], false);

    logger.record_incoming(&input());

    let lines = parse_lines(&read_log(&path));
    assert_eq!(lines.len(), 1);
    let rec = &lines[0];
    assert_eq!(rec["event"], "incoming");
    assert_eq!(rec["agent_id"], "agent-1");
    assert_eq!(rec["action"], "Bash");
    assert_eq!(rec["working_dir"], "/tmp");
    assert!(rec["ts"].as_str().is_some_and(|s| !s.is_empty()));
}

#[test]
fn record_incoming_carries_markers_as_field() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger =
        FileClassificationHistoryWriter::new(path.clone(), vec!["dev-bypass-active".into()], false);

    logger.record_incoming(&input());

    let rec = &parse_lines(&read_log(&path))[0];
    assert_eq!(rec["markers"], serde_json::json!(["dev-bypass-active"]));
}

#[test]
fn record_incoming_omits_markers_when_empty() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(path.clone(), vec![], false);

    logger.record_incoming(&input());

    let rec = &parse_lines(&read_log(&path))[0];
    assert!(rec.get("markers").is_none(), "markers omitted when empty");
}

#[test]
fn record_incoming_omits_args_by_default() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(path.clone(), vec![], false);

    logger.record_incoming(&input());

    let rec = &parse_lines(&read_log(&path))[0];
    assert!(
        rec.get("args").is_none(),
        "args omitted when debug_mode off"
    );
}

#[test]
fn record_incoming_includes_args_when_debug_mode() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let logger = FileClassificationHistoryWriter::new(path.clone(), vec![], true);

    logger.record_incoming(&input());

    let rec = &parse_lines(&read_log(&path))[0];
    assert_eq!(rec["args"], "{\"command\":\"ls\"}");
}
