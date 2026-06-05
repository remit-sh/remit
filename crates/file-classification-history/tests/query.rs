use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use domain::{
    ActionKind, AgentId, ClassificationHistoryReader, ClassificationHistoryWriter,
    ClassificationRecord, CorrelationId, Enforced, HistoryQuery, RedKind, Verdict, VerdictFilter,
};
use file_classification_history::{
    FileClassificationHistoryReader, FileClassificationHistoryWriter,
};
use tempfile::tempdir;

fn make(
    session: &str,
    parent: Option<&str>,
    verdict: Verdict,
    action: &str,
    secs_ago: u64,
) -> ClassificationRecord {
    let now = time::OffsetDateTime::now_utc();
    let ts = now - Duration::from_secs(secs_ago);
    let enforced = match &verdict {
        Verdict::Green => Enforced::Permit,
        Verdict::Red { .. } => Enforced::Deny,
    };
    ClassificationRecord {
        ts,
        session_id: AgentId::try_new(session).unwrap(),
        parent_id: parent.map(|p| AgentId::try_new(p).unwrap()),
        agent_type: None,
        working_dir: PathBuf::from("/tmp"),
        call_id: None,
        correlation_id: CorrelationId::new(),
        pid: 1,
        hook_duration: Duration::from_millis(1),
        tool_name: ActionKind::new(action),
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
        verdict,
        enforced,
        override_action: None,
        error_chain: None,
        dev_bypass: false,
        details: BTreeMap::new(),
    }
}

fn green(session: &str, parent: Option<&str>) -> ClassificationRecord {
    make(session, parent, Verdict::Green, "Bash", 0)
}

fn red(session: &str) -> ClassificationRecord {
    make(
        session,
        None,
        Verdict::Red {
            kind: RedKind::OutOfScope,
            reason: "nope".into(),
        },
        "Bash",
        0,
    )
}

#[test]
fn empty_log_returns_no_records() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let reader = FileClassificationHistoryReader::new(path);
    assert!(reader.query(&HistoryQuery::default()).unwrap().is_empty());
}

#[test]
fn agent_id_filter_returns_only_matching_session() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());
    writer.write(green("a", None)).unwrap();
    writer.write(green("b", None)).unwrap();

    let reader = FileClassificationHistoryReader::new(path);
    let q = HistoryQuery {
        agent_id: Some(AgentId::try_new("a").unwrap()),
        ..Default::default()
    };
    let results = reader.query(&q).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id.as_ref(), "a");
}

#[test]
fn verdict_filter_separates_green_and_red() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());
    writer.write(green("a", None)).unwrap();
    writer.write(red("b")).unwrap();

    let reader = FileClassificationHistoryReader::new(path);
    let q = HistoryQuery {
        verdict: Some(VerdictFilter::Red),
        ..Default::default()
    };
    let results = reader.query(&q).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id.as_ref(), "b");
}

#[test]
fn action_filter_matches_tool_name() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());
    writer
        .write(make("a", None, Verdict::Green, "Bash", 0))
        .unwrap();
    writer
        .write(make("b", None, Verdict::Green, "Read", 0))
        .unwrap();

    let reader = FileClassificationHistoryReader::new(path);
    let q = HistoryQuery {
        action: Some(ActionKind::new("Bash")),
        ..Default::default()
    };
    let results = reader.query(&q).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_name.as_ref(), "Bash");
}

#[test]
fn since_filter_drops_older_records() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());
    writer
        .write(make("old", None, Verdict::Green, "Bash", 600))
        .unwrap();
    writer
        .write(make("new", None, Verdict::Green, "Bash", 5))
        .unwrap();

    let reader = FileClassificationHistoryReader::new(path);
    let q = HistoryQuery {
        since: Some(std::time::SystemTime::now() - Duration::from_secs(60)),
        ..Default::default()
    };
    let results = reader.query(&q).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id.as_ref(), "new");
}

#[test]
fn limit_returns_most_recent_after_filter() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());
    for i in 0..5 {
        writer
            .write(make(
                &format!("sess-{i}"),
                None,
                Verdict::Green,
                "Bash",
                (5 - i) * 10,
            ))
            .unwrap();
    }

    let reader = FileClassificationHistoryReader::new(path);
    let q = HistoryQuery {
        limit: Some(2),
        ..Default::default()
    };
    let results = reader.query(&q).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].session_id.as_ref(), "sess-3");
    assert_eq!(results[1].session_id.as_ref(), "sess-4");
}
