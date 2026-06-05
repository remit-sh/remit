use std::collections::HashSet;

use crate::{
    AgentId, AgentStore, ClassificationHistoryReader, ClassificationRecord, HistoryQuery,
    HistoryQueryError, Scope, Verdict, VerdictFilter,
};

/// Contract test for [`AgentStore`] implementations. Adapters call this from
/// a `#[test]` to prove conformance — same suite for every backend. Takes a
/// factory closure so persistence is exercised across fresh instances.
///
/// Covers the three concerns of [`AgentStore`]:
/// - Scope: round-trip, overwrite, independence per agent, persistence.
/// - Bundled scope staging: single-shot `take`, overwrite-before-take semantics
///   (single-slot in MVP), sequential cycles, per-parent independence,
///   persistence.
/// - Disable: monotonic marker, per-agent independence, persistence.
pub fn agent_store_contract<S, F>(make: F)
where
    S: AgentStore,
    F: Fn() -> S,
{
    let id_a = AgentId::try_new("contract-test-agent-a").unwrap();
    let id_b = AgentId::try_new("contract-test-agent-b").unwrap();
    let scope_1 = Scope::try_new("first scope").unwrap();
    let scope_2 = Scope::try_new("second scope").unwrap();
    // 8 KiB matches the Scope newtype cap.
    let long = Scope::try_new("x".repeat(8 * 1024)).unwrap();
    let unicode = Scope::try_new("范围 — naïve café 🦀").unwrap();

    // ----- Scope round-trip + overwrite -----
    let store = make();

    assert!(matches!(store.scope_for(&id_a), Ok(None)));

    store.assign_scope(&id_a, scope_1.clone()).unwrap();
    assert_eq!(store.scope_for(&id_a).unwrap(), Some(scope_1));

    store.assign_scope(&id_a, scope_2.clone()).unwrap();
    assert_eq!(store.scope_for(&id_a).unwrap(), Some(scope_2.clone()));

    assert!(matches!(store.scope_for(&id_b), Ok(None)));

    store.assign_scope(&id_a, long.clone()).unwrap();
    assert_eq!(store.scope_for(&id_a).unwrap(), Some(long));

    store.assign_scope(&id_b, unicode.clone()).unwrap();
    assert_eq!(store.scope_for(&id_b).unwrap(), Some(unicode));

    // ----- Pending bundled-scope staging -----
    let parent_a = AgentId::try_new("contract-test-parent-a").unwrap();
    let parent_b = AgentId::try_new("contract-test-parent-b").unwrap();
    let pending_1 = Scope::try_new("pending first").unwrap();
    let pending_2 = Scope::try_new("pending second").unwrap();

    // Empty → take returns Ok(None).
    assert!(matches!(store.take_pending(&parent_a), Ok(None)));

    // set then take returns the scope, then a second take returns None (single-shot).
    store.set_pending(&parent_a, pending_1.clone()).unwrap();
    assert_eq!(
        store.take_pending(&parent_a).unwrap(),
        Some(pending_1.clone())
    );
    assert!(matches!(store.take_pending(&parent_a), Ok(None)));

    // set; set; take → second scope (overwrite-before-take pinned for the future
    // FIFO migration tracked as #20).
    store.set_pending(&parent_a, pending_1.clone()).unwrap();
    store.set_pending(&parent_a, pending_2.clone()).unwrap();
    assert_eq!(
        store.take_pending(&parent_a).unwrap(),
        Some(pending_2.clone())
    );

    // Sequential cycles from the same parent (parent spawns several children
    // one after another).
    store.set_pending(&parent_a, pending_1.clone()).unwrap();
    assert_eq!(
        store.take_pending(&parent_a).unwrap(),
        Some(pending_1.clone())
    );
    store.set_pending(&parent_a, pending_2.clone()).unwrap();
    assert_eq!(
        store.take_pending(&parent_a).unwrap(),
        Some(pending_2.clone())
    );

    // Independence: taking parent A's row doesn't affect parent B's.
    store.set_pending(&parent_a, pending_1.clone()).unwrap();
    store.set_pending(&parent_b, pending_2.clone()).unwrap();
    assert_eq!(
        store.take_pending(&parent_a).unwrap(),
        Some(pending_1.clone())
    );
    assert_eq!(
        store.take_pending(&parent_b).unwrap(),
        Some(pending_2.clone())
    );

    // ----- Disable marker -----
    let disable_a = AgentId::try_new("contract-test-disable-a").unwrap();
    let disable_b = AgentId::try_new("contract-test-disable-b").unwrap();
    assert!(!store.is_disabled(&disable_a).unwrap());
    store.disable(&disable_a).unwrap();
    assert!(store.is_disabled(&disable_a).unwrap());
    // Independence: disabling one agent does not disable another.
    assert!(!store.is_disabled(&disable_b).unwrap());
    // Idempotence: a second disable() on the same id is a no-op (no error).
    store.disable(&disable_a).unwrap();
    assert!(store.is_disabled(&disable_a).unwrap());

    // ----- Persistence across fresh instances -----
    let persistent_pending = Scope::try_new("persistent pending").unwrap();
    store
        .set_pending(&parent_a, persistent_pending.clone())
        .unwrap();
    drop(store);
    let store = make();
    assert!(store.scope_for(&id_a).unwrap().is_some());
    assert!(store.scope_for(&id_b).unwrap().is_some());
    assert_eq!(
        store.take_pending(&parent_a).unwrap(),
        Some(persistent_pending)
    );
    assert!(store.is_disabled(&disable_a).unwrap());
    assert!(!store.is_disabled(&disable_b).unwrap());
}

/// In-memory [`ClassificationHistoryReader`] for domain tests and
/// downstream adapter tests. Records are matched against
/// [`HistoryQuery`] in append order; root-descendant resolution walks
/// the parent-agent chain reconstructed from the records themselves.
#[derive(Clone, Debug, Default)]
pub struct InMemoryClassificationHistory {
    records: Vec<ClassificationRecord>,
}

impl InMemoryClassificationHistory {
    pub fn new(records: Vec<ClassificationRecord>) -> Self {
        Self { records }
    }

    pub fn push(&mut self, record: ClassificationRecord) {
        self.records.push(record);
    }
}

impl ClassificationHistoryReader for InMemoryClassificationHistory {
    fn query(&self, query: &HistoryQuery) -> Result<Vec<ClassificationRecord>, HistoryQueryError> {
        let matches: Vec<ClassificationRecord> = self
            .records
            .iter()
            .filter(|r| record_matches(r, query, &self.records))
            .cloned()
            .collect();
        Ok(apply_limit(matches, query.limit))
    }
}

fn record_matches(
    record: &ClassificationRecord,
    query: &HistoryQuery,
    all: &[ClassificationRecord],
) -> bool {
    if let Some(agent_id) = &query.agent_id
        && &record.session_id != agent_id
    {
        return false;
    }
    if let Some(root_agent_id) = &query.root_agent_id
        && !is_descendant_of(&record.session_id, root_agent_id, all)
    {
        return false;
    }
    if let Some(filter) = query.verdict
        && !verdict_matches(filter, &record.verdict)
    {
        return false;
    }
    if let Some(since) = query.since
        && std::time::SystemTime::from(record.ts) < since
    {
        return false;
    }
    if let Some(action) = &query.action
        && &record.tool_name != action
    {
        return false;
    }
    true
}

fn verdict_matches(filter: VerdictFilter, verdict: &Verdict) -> bool {
    matches!(
        (filter, verdict),
        (VerdictFilter::Green, Verdict::Green) | (VerdictFilter::Red, Verdict::Red { .. })
    )
}

/// Walk `agent`'s parent-agent chain via the records. Returns true iff
/// `root` is `agent` itself or any ancestor. Cycle-safe.
fn is_descendant_of(agent: &AgentId, root: &AgentId, all: &[ClassificationRecord]) -> bool {
    if agent == root {
        return true;
    }
    let mut visited: HashSet<&AgentId> = HashSet::new();
    let mut current = agent;
    while visited.insert(current) {
        let Some(rec) = all.iter().find(|r| &r.session_id == current) else {
            return false;
        };
        match &rec.parent_id {
            Some(parent) if parent == root => return true,
            Some(parent) => current = parent,
            None => return false,
        }
    }
    false
}

/// `Some(0)` is treated as unlimited. Otherwise returns the most-recent
/// (last) N records, preserving append order.
fn apply_limit(
    mut records: Vec<ClassificationRecord>,
    limit: Option<usize>,
) -> Vec<ClassificationRecord> {
    if let Some(n) = limit.filter(|&n| n > 0)
        && records.len() > n
    {
        let total = records.len();
        records.drain(..total - n);
    }
    records
}
