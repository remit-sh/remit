use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use domain::{
    ActionKind, AgentId, ClassificationHistoryReader, HistoryQuery, HistoryQueryError, Verdict,
    VerdictFilter,
};
use file_classification_history::FileClassificationHistoryReader;
use tempfile::tempdir;

fn write_log(path: &PathBuf, lines: &[&str]) {
    fs::write(path, lines.join("\n") + "\n").unwrap();
}

struct Out<'a> {
    ts: &'a str,
    agent_id: &'a str,
    parent_id: Option<&'a str>,
    action: &'a str,
    verdict: &'a str,
    extras: &'a str,
}

fn outcome(o: Out<'_>) -> String {
    let parent = o
        .parent_id
        .map(|p| format!(r#","parent_id":"{p}""#))
        .unwrap_or_default();
    format!(
        r#"{{"ts":"{}","event":"outcome","agent_id":"{}"{parent},"action":"{}","working_dir":"/tmp","verdict":"{}"{}}}"#,
        o.ts, o.agent_id, o.action, o.verdict, o.extras,
    )
}

fn green(ts: &str, agent: &str, action: &str) -> String {
    outcome(Out {
        ts,
        agent_id: agent,
        parent_id: None,
        action,
        verdict: "green",
        extras: "",
    })
}

fn green_with_parent(ts: &str, agent: &str, parent: &str, action: &str) -> String {
    outcome(Out {
        ts,
        agent_id: agent,
        parent_id: Some(parent),
        action,
        verdict: "green",
        extras: "",
    })
}

#[test]
fn missing_file_returns_empty_result() {
    let dir = tempdir().unwrap();
    let log = FileClassificationHistoryReader::new(dir.path().join("does-not-exist.log"));

    let results = log.query(&HistoryQuery::default()).unwrap();
    assert!(results.is_empty());
}

#[test]
fn empty_file_returns_empty_result() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    fs::write(&path, "").unwrap();
    let log = FileClassificationHistoryReader::new(path);

    let results = log.query(&HistoryQuery::default()).unwrap();
    assert!(results.is_empty());
}

#[test]
fn malformed_line_returns_unavailable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    write_log(&path, &["{this is not json"]);
    let log = FileClassificationHistoryReader::new(path);

    let err = log.query(&HistoryQuery::default()).unwrap_err();
    assert!(matches!(err, HistoryQueryError::Unavailable(_)));
}

#[test]
fn only_outcome_lines_materialise_as_records() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    write_log(
        &path,
        &[
            &green("2026-05-12T10:14:33Z", "a", "Bash"),
            r#"{"ts":"2026-05-12T10:14:34Z","event":"incoming","agent_id":"a","action":"Read","working_dir":"/tmp"}"#,
            r#"{"ts":"2026-05-12T10:14:35Z","event":"hook","name":"PreToolUse"}"#,
            r#"{"ts":"2026-05-12T10:14:36Z","event":"stage_pending","parent":"a","outcome":"set"}"#,
        ],
    );
    let log = FileClassificationHistoryReader::new(path);

    let results = log.query(&HistoryQuery::default()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].agent_id, AgentId::try_new("a").unwrap());
    assert_eq!(results[0].action, ActionKind::new("Bash"));
}

#[test]
fn unknown_event_types_are_ignored() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    write_log(
        &path,
        &[
            r#"{"ts":"2026-05-12T10:14:33Z","event":"future_unknown_thing","payload":42}"#,
            &green("2026-05-12T10:14:34Z", "a", "Bash"),
        ],
    );
    let log = FileClassificationHistoryReader::new(path);

    let results = log.query(&HistoryQuery::default()).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn red_outcome_carries_kind_and_reason() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let red = outcome(Out {
        ts: "2026-05-12T10:14:33Z",
        agent_id: "a",
        parent_id: None,
        action: "Write",
        verdict: "red",
        extras: r#","red_kind":"OutOfScope","reason":"writes outside scope""#,
    });
    write_log(&path, &[&red]);
    let log = FileClassificationHistoryReader::new(path);

    let results = log.query(&HistoryQuery::default()).unwrap();
    let r = &results[0];
    match &r.verdict {
        Verdict::Red { reason, .. } => {
            assert_eq!(reason, "writes outside scope");
        }
        _ => panic!("expected red"),
    }
}

#[test]
fn returns_records_in_file_order() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    write_log(
        &path,
        &[
            &green("2026-05-12T10:14:33Z", "a", "Bash"),
            &green("2026-05-12T10:14:34Z", "a", "Write"),
            &green("2026-05-12T10:14:35Z", "a", "Read"),
        ],
    );
    let log = FileClassificationHistoryReader::new(path);

    let results = log.query(&HistoryQuery::default()).unwrap();
    let actions: Vec<_> = results.iter().map(|r| r.action.as_ref()).collect();
    assert_eq!(actions, vec!["Bash", "Write", "Read"]);
}

#[test]
fn root_descendant_walk_uses_incoming_records() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    // Parent emits no outcome, only an incoming. Child outcomes should
    // still resolve to root=parent via the parent map built from
    // incoming records.
    write_log(
        &path,
        &[
            r#"{"ts":"2026-05-12T10:14:33Z","event":"incoming","agent_id":"child","parent_id":"parent","action":"Bash","working_dir":"/tmp"}"#,
            &green_with_parent("2026-05-12T10:14:34Z", "child", "parent", "Write"),
        ],
    );
    let log = FileClassificationHistoryReader::new(path);

    let results = log
        .query(&HistoryQuery {
            root_agent_id: Some(AgentId::try_new("parent").unwrap()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].agent_id.as_ref(), "child");
}

#[test]
fn filters_apply_to_results() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let red = outcome(Out {
        ts: "2026-05-12T10:14:34Z",
        agent_id: "a",
        parent_id: None,
        action: "Write",
        verdict: "red",
        extras: r#","red_kind":"OutOfScope","reason":"x""#,
    });
    write_log(
        &path,
        &[
            &green("2026-05-12T10:14:33Z", "a", "Bash"),
            &red,
            &green("2026-05-12T10:14:35Z", "b", "Bash"),
        ],
    );
    let log = FileClassificationHistoryReader::new(path);

    let results = log
        .query(&HistoryQuery {
            verdict: Some(VerdictFilter::Red),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].agent_id.as_ref(), "a");
}

#[test]
fn since_filter_parses_record_timestamps() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    write_log(
        &path,
        &[
            &green("2026-05-12T10:14:33Z", "a", "Bash"),
            &green("2026-05-12T10:14:35Z", "a", "Bash"),
        ],
    );
    let log = FileClassificationHistoryReader::new(path);

    // 2026-05-12T10:14:34Z is between the two records.
    let between = UNIX_EPOCH + Duration::from_secs(1778580874);
    let between = std::cmp::max(between, SystemTime::UNIX_EPOCH);

    let results = log
        .query(&HistoryQuery {
            since: Some(between),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn args_round_trip_for_debug_mode_records() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let line = outcome(Out {
        ts: "2026-05-12T10:14:33Z",
        agent_id: "a",
        parent_id: None,
        action: "Bash",
        verdict: "green",
        extras: r#","args":"{\"command\":\"ls\"}""#,
    });
    write_log(&path, &[&line]);
    let log = FileClassificationHistoryReader::new(path);

    let results = log.query(&HistoryQuery::default()).unwrap();
    assert_eq!(
        results[0]
            .args
            .as_ref()
            .map(|a| a.as_ref())
            .unwrap_or_default(),
        "{\"command\":\"ls\"}"
    );
}

#[test]
fn missing_args_field_yields_none() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    write_log(&path, &[&green("2026-05-12T10:14:33Z", "a", "Bash")]);
    let log = FileClassificationHistoryReader::new(path);

    let results = log.query(&HistoryQuery::default()).unwrap();
    assert!(results[0].args.is_none());
}

#[test]
fn invalid_ts_returns_unavailable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    write_log(&path, &[&green("not-a-timestamp", "a", "Bash")]);
    let log = FileClassificationHistoryReader::new(path);

    let err = log.query(&HistoryQuery::default()).unwrap_err();
    assert!(matches!(err, HistoryQueryError::Unavailable(_)));
}
