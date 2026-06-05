#![cfg(feature = "test-support")]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use domain::test_support::InMemoryClassificationHistory;
use domain::{
    ActionKind, AgentId, ClassificationHistoryReader, ClassificationRecord, CorrelationId,
    Enforced, HistoryQuery, RedKind, Verdict, VerdictFilter,
};

fn ts(seconds: u64) -> time::OffsetDateTime {
    let st = UNIX_EPOCH + Duration::from_secs(seconds);
    time::OffsetDateTime::from(st)
}

fn agent(id: &str) -> AgentId {
    AgentId::try_new(id).unwrap()
}

fn rec(
    seconds: u64,
    id: &str,
    parent: Option<&str>,
    action: &str,
    verdict: Verdict,
) -> ClassificationRecord {
    let enforced = match &verdict {
        Verdict::Green => Enforced::Permit,
        Verdict::Red { .. } => Enforced::Deny,
    };
    ClassificationRecord {
        ts: ts(seconds),
        session_id: agent(id),
        parent_id: parent.map(agent),
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

fn green() -> Verdict {
    Verdict::Green
}

fn red() -> Verdict {
    Verdict::Red {
        kind: RedKind::OutOfScope,
        reason: "out of scope".into(),
    }
}

#[test]
fn empty_query_returns_all_records() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "a", None, "Write", red()),
    ]);

    let results = log.query(&HistoryQuery::default()).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn filters_by_agent_id() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "b", None, "Bash", green()),
        rec(3, "a", None, "Write", red()),
    ]);

    let results = log
        .query(&HistoryQuery {
            agent_id: Some(agent("a")),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.session_id == agent("a")));
}

#[test]
fn filters_by_verdict_green() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "a", None, "Write", red()),
        rec(3, "a", None, "Read", green()),
    ]);

    let results = log
        .query(&HistoryQuery {
            verdict: Some(VerdictFilter::Green),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| matches!(r.verdict, Verdict::Green)));
}

#[test]
fn filters_by_verdict_red() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "a", None, "Write", red()),
        rec(3, "a", None, "Read", green()),
    ]);

    let results = log
        .query(&HistoryQuery {
            verdict: Some(VerdictFilter::Red),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(matches!(results[0].verdict, Verdict::Red { .. }));
}

#[test]
fn filters_by_action() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "a", None, "Write", green()),
        rec(3, "a", None, "Bash", red()),
    ]);

    let results = log
        .query(&HistoryQuery {
            action: Some(ActionKind::new("Bash")),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.tool_name.as_ref() == "Bash"));
}

#[test]
fn since_boundary_is_inclusive() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "a", None, "Bash", green()),
        rec(3, "a", None, "Bash", green()),
    ]);

    let results = log
        .query(&HistoryQuery {
            since: Some(SystemTime::from(ts(2))),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].ts, ts(2));
}

#[test]
fn filters_compose() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "a", None, "Write", red()),
        rec(3, "b", None, "Write", red()),
        rec(4, "a", None, "Bash", red()),
    ]);

    let results = log
        .query(&HistoryQuery {
            agent_id: Some(agent("a")),
            verdict: Some(VerdictFilter::Red),
            action: Some(ActionKind::new("Write")),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id, agent("a"));
    assert_eq!(results[0].tool_name.as_ref(), "Write");
}

#[test]
fn root_resolves_to_descendants_inclusive() {
    // tree: root -> child-1 -> grandchild ; root -> child-2
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "root", None, "Bash", green()),
        rec(2, "child-1", Some("root"), "Read", green()),
        rec(3, "grandchild", Some("child-1"), "Write", red()),
        rec(4, "child-2", Some("root"), "Bash", green()),
        rec(5, "outsider", None, "Bash", green()),
    ]);

    let results = log
        .query(&HistoryQuery {
            root_agent_id: Some(agent("root")),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 4);
    let ids: Vec<_> = results.iter().map(|r| r.session_id.as_ref()).collect();
    assert!(ids.contains(&"root"));
    assert!(ids.contains(&"child-1"));
    assert!(ids.contains(&"grandchild"));
    assert!(ids.contains(&"child-2"));
}

#[test]
fn root_does_not_match_siblings_or_unrelated_agents() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "b", None, "Bash", green()),
    ]);

    let results = log
        .query(&HistoryQuery {
            root_agent_id: Some(agent("a")),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id, agent("a"));
}

#[test]
fn limit_returns_most_recent_n() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "a", None, "Write", green()),
        rec(3, "a", None, "Read", green()),
        rec(4, "a", None, "Bash", green()),
        rec(5, "a", None, "Write", green()),
    ]);

    let results = log
        .query(&HistoryQuery {
            limit: Some(2),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].ts, ts(4));
    assert_eq!(results[1].ts, ts(5));
}

#[test]
fn limit_zero_is_unlimited() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "a", None, "Bash", green()),
        rec(3, "a", None, "Bash", green()),
    ]);

    let results = log
        .query(&HistoryQuery {
            limit: Some(0),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn limit_applies_after_filtering() {
    let log = InMemoryClassificationHistory::new(vec![
        rec(1, "a", None, "Bash", green()),
        rec(2, "a", None, "Write", red()),
        rec(3, "a", None, "Bash", red()),
        rec(4, "a", None, "Read", green()),
        rec(5, "a", None, "Write", red()),
    ]);

    let results = log
        .query(&HistoryQuery {
            verdict: Some(VerdictFilter::Red),
            limit: Some(2),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].ts, ts(3));
    assert_eq!(results[1].ts, ts(5));
}
