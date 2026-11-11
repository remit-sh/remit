use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use domain::{
    ActionKind, AgentId, AgentType, CallId, ClassificationEvent, ClassificationHistoryReader,
    ClassificationHistoryWriter, ClassificationRecord, CorrelationId, Enforced, EventKind,
    HistoryQuery, OverrideAction, RawVerdict, RecordedClassifierOutcome, RecordedLlmCallStats,
    RedKind, Scope, ScopeOrigin, Verdict,
};
use tempfile::tempdir;

use super::*;

fn make_record(session: &str, parent: Option<&str>) -> ClassificationRecord {
    ClassificationRecord {
        ts: time::OffsetDateTime::now_utc(),
        session_id: AgentId::try_new(session).unwrap(),
        parent_id: parent.map(|p| AgentId::try_new(p).unwrap()),
        agent_type: None,
        working_dir: PathBuf::from("/tmp"),
        call_id: None,
        correlation_id: CorrelationId::new(),
        pid: 1234,
        hook_duration: Duration::from_millis(10),
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
fn write_then_read_round_trips_classification() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());

    let record = make_record("session-a", None);
    writer.write(record.clone()).unwrap();

    let reader = FileClassificationHistoryReader::new(path);
    let results = reader.query(&HistoryQuery::default()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id, record.session_id);
    assert_eq!(results[0].verdict, Verdict::Green);
    assert_eq!(results[0].enforced, Enforced::Permit);
}

#[test]
fn write_event_does_not_appear_in_query_results() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());

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
    writer.write(make_record("session-a", None)).unwrap();

    let reader = FileClassificationHistoryReader::new(path);
    let results = reader.query(&HistoryQuery::default()).unwrap();
    assert_eq!(results.len(), 1, "events are skipped during query");
}

#[test]
fn root_agent_id_filter_uses_parent_chain() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());

    writer.write(make_record("root", None)).unwrap();
    writer.write(make_record("child", Some("root"))).unwrap();
    writer
        .write(make_record("grandchild", Some("child")))
        .unwrap();
    writer.write(make_record("unrelated", None)).unwrap();

    let reader = FileClassificationHistoryReader::new(path);
    let q = HistoryQuery {
        root_agent_id: Some(AgentId::try_new("root").unwrap()),
        ..Default::default()
    };
    let results = reader.query(&q).unwrap();
    let ids: Vec<String> = results
        .iter()
        .map(|r| r.session_id.as_ref().to_string())
        .collect();
    assert!(ids.contains(&"root".into()));
    assert!(ids.contains(&"child".into()));
    assert!(ids.contains(&"grandchild".into()));
    assert!(!ids.contains(&"unrelated".into()));
}

#[test]
fn fully_populated_record_round_trips() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());

    let correlation = CorrelationId::new();
    let scope = Scope::try_new("do the thing").unwrap();
    let mut details = BTreeMap::new();
    details.insert("hint".into(), "extra".into());

    let record = ClassificationRecord {
        ts: time::OffsetDateTime::now_utc(),
        session_id: AgentId::try_new("sess").unwrap(),
        parent_id: Some(AgentId::try_new("parent").unwrap()),
        agent_type: Some(AgentType::try_new("Plan").unwrap()),
        working_dir: PathBuf::from("/Users/a/dev/foo"),
        call_id: Some(CallId::new("tu-1")),
        correlation_id: correlation.clone(),
        pid: 4242,
        hook_duration: Duration::from_millis(412),
        tool_name: ActionKind::new("Bash"),
        tool_input: Some(r#"{"command":"ls"}"#.into()),
        bundled_scope_present: true,
        narration_fields_stripped: 2,
        args_size_pre: 87,
        args_size_post: 32,
        scope: Some(scope.clone()),
        scope_origin: Some(ScopeOrigin::Edited),
        llm_outcome: Some(RecordedClassifierOutcome {
            raw_verdict: RawVerdict::Red,
            raw_reason: Some("model said so".into()),
            llm: Some(RecordedLlmCallStats {
                model: "claude-haiku-4-5".into(),
                latency: Duration::from_millis(312),
                input_tokens: Some(234),
                output_tokens: Some(12),
            }),
        }),
        prompt_shown: true,
        prompt_latency: Some(Duration::from_secs(8)),
        lock_wait: Some(Duration::from_millis(40)),
        initial_verdict: Some(Verdict::Red {
            kind: RedKind::OutOfScope,
            reason: "first call said no".into(),
        }),
        verdict: Verdict::Green,
        enforced: Enforced::Permit,
        override_action: Some(OverrideAction::UserAllow),
        error_chain: Some("io: file not found".into()),
        dev_bypass: false,
        details: details.clone(),
    };

    writer.write(record.clone()).unwrap();

    let reader = FileClassificationHistoryReader::new(path);
    let results = reader.query(&HistoryQuery::default()).unwrap();
    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert_eq!(r.session_id, record.session_id);
    assert_eq!(r.parent_id, record.parent_id);
    assert_eq!(r.agent_type, record.agent_type);
    assert_eq!(r.working_dir, record.working_dir);
    assert_eq!(
        r.call_id.as_ref().map(|c| c.as_ref().to_string()),
        Some("tu-1".to_string())
    );
    assert_eq!(r.correlation_id, correlation);
    assert_eq!(r.pid, 4242);
    assert_eq!(r.hook_duration, Duration::from_millis(412));
    assert_eq!(r.tool_input.as_deref(), Some(r#"{"command":"ls"}"#));
    assert!(r.bundled_scope_present);
    assert_eq!(r.narration_fields_stripped, 2);
    assert_eq!(r.scope.as_ref(), Some(&scope));
    assert_eq!(r.scope_origin, Some(ScopeOrigin::Edited));
    assert!(r.prompt_shown);
    assert_eq!(r.prompt_latency, Some(Duration::from_secs(8)));
    assert_eq!(r.lock_wait, Some(Duration::from_millis(40)));
    assert_eq!(r.verdict, Verdict::Green);
    assert_eq!(r.initial_verdict, record.initial_verdict);
    assert_eq!(r.enforced, Enforced::Permit);
    assert_eq!(r.override_action, Some(OverrideAction::UserAllow));
    assert_eq!(r.details, details);
    let llm = r.llm_outcome.as_ref().expect("llm_outcome");
    assert_eq!(llm.raw_verdict, RawVerdict::Red);
    assert_eq!(llm.raw_reason.as_deref(), Some("model said so"));
    let stats = llm.llm.as_ref().expect("llm stats");
    assert_eq!(stats.model, "claude-haiku-4-5");
    assert_eq!(stats.latency, Duration::from_millis(312));
    assert_eq!(stats.input_tokens, Some(234));
    assert_eq!(stats.output_tokens, Some(12));
}

#[test]
fn all_scope_origin_variants_round_trip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());

    for (idx, origin) in [
        ScopeOrigin::PreExisting,
        ScopeOrigin::ClaimedFromPending,
        ScopeOrigin::Fresh,
        ScopeOrigin::Edited,
        ScopeOrigin::Extracted,
    ]
    .iter()
    .enumerate()
    {
        let mut r = make_record(&format!("sess-{idx}"), None);
        r.scope_origin = Some(*origin);
        writer.write(r).unwrap();
    }

    let reader = FileClassificationHistoryReader::new(path);
    let results = reader.query(&HistoryQuery::default()).unwrap();
    let origins: Vec<Option<ScopeOrigin>> = results.iter().map(|r| r.scope_origin).collect();
    assert_eq!(
        origins,
        vec![
            Some(ScopeOrigin::PreExisting),
            Some(ScopeOrigin::ClaimedFromPending),
            Some(ScopeOrigin::Fresh),
            Some(ScopeOrigin::Edited),
            Some(ScopeOrigin::Extracted),
        ]
    );
}

#[test]
fn malformed_lines_are_skipped_not_fatal() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("remit.log");
    let writer = FileClassificationHistoryWriter::new(path.clone());

    writer.write(make_record("a", None)).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"this is not json\n")
        .unwrap();
    writer.write(make_record("b", None)).unwrap();

    let reader = FileClassificationHistoryReader::new(path);
    let results = reader.query(&HistoryQuery::default()).unwrap();
    assert_eq!(
        results.len(),
        2,
        "valid rows survive a malformed line in between"
    );
}
