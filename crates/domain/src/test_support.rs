use crate::{AgentId, PendingScopeStore, Scope, ScopeReader, ScopeWriter};

/// Contract test for `ScopeReader` + `ScopeWriter` implementations.
/// Adapters call this from a `#[test]` to prove conformance — same suite for every backend.
/// Takes a factory closure so persistence can be tested across fresh instances.
pub fn scope_store_contract<S, F>(make: F)
where
    S: ScopeReader + ScopeWriter,
    F: Fn() -> S,
{
    let id_a = AgentId::try_new("contract-test-agent-a").unwrap();
    let id_b = AgentId::try_new("contract-test-agent-b").unwrap();
    let scope_1 = Scope::try_new("first scope").unwrap();
    let scope_2 = Scope::try_new("second scope").unwrap();
    let long = Scope::try_new("x".repeat(64 * 1024)).unwrap();
    let unicode = Scope::try_new("范围 — naïve café 🦀").unwrap();

    let store = make();

    assert!(matches!(store.scope_for(&id_a), Ok(None)));

    store.assign(&id_a, scope_1.clone()).unwrap();
    assert_eq!(store.scope_for(&id_a).unwrap(), Some(scope_1));

    store.assign(&id_a, scope_2.clone()).unwrap();
    assert_eq!(store.scope_for(&id_a).unwrap(), Some(scope_2.clone()));

    assert!(matches!(store.scope_for(&id_b), Ok(None)));

    store.assign(&id_a, long.clone()).unwrap();
    assert_eq!(store.scope_for(&id_a).unwrap(), Some(long));

    store.assign(&id_b, unicode.clone()).unwrap();
    assert_eq!(store.scope_for(&id_b).unwrap(), Some(unicode));

    drop(store);
    let store = make();
    assert!(store.scope_for(&id_a).unwrap().is_some());
    assert!(store.scope_for(&id_b).unwrap().is_some());
}

/// Contract test for `PendingScopeStore` implementations.
/// Pins single-shot semantics (`take` consumes), independence between parents, and
/// sequential stage/take cycles from the same parent.
pub fn pending_scope_store_contract<S, F>(make: F)
where
    S: PendingScopeStore,
    F: Fn() -> S,
{
    let parent_a = AgentId::try_new("contract-test-parent-a").unwrap();
    let parent_b = AgentId::try_new("contract-test-parent-b").unwrap();
    let scope_1 = Scope::try_new("first").unwrap();
    let scope_2 = Scope::try_new("second").unwrap();

    let store = make();

    // Empty store → take returns Ok(None).
    assert!(matches!(store.take(&parent_a), Ok(None)));

    // set then take returns the scope, then a second take returns None (single-shot).
    store.set(&parent_a, scope_1.clone()).unwrap();
    assert_eq!(store.take(&parent_a).unwrap(), Some(scope_1.clone()));
    assert!(matches!(store.take(&parent_a), Ok(None)));

    // set; set; take → second scope (overwrite-before-take is the documented
    // single-slot loss; tests pin it so a future FIFO migration sees the change.)
    store.set(&parent_a, scope_1.clone()).unwrap();
    store.set(&parent_a, scope_2.clone()).unwrap();
    assert_eq!(store.take(&parent_a).unwrap(), Some(scope_2.clone()));

    // Sequential stage/take cycles from the same parent (the common pattern when a
    // parent spawns multiple children one after another).
    store.set(&parent_a, scope_1.clone()).unwrap();
    assert_eq!(store.take(&parent_a).unwrap(), Some(scope_1.clone()));
    store.set(&parent_a, scope_2.clone()).unwrap();
    assert_eq!(store.take(&parent_a).unwrap(), Some(scope_2.clone()));

    // Independence: taking parent A's row doesn't affect parent B's.
    store.set(&parent_a, scope_1.clone()).unwrap();
    store.set(&parent_b, scope_2.clone()).unwrap();
    assert_eq!(store.take(&parent_a).unwrap(), Some(scope_1));
    assert_eq!(store.take(&parent_b).unwrap(), Some(scope_2));

    // Persistence across fresh instances: stage, drop, recreate, take.
    let new_scope = Scope::try_new("persistent").unwrap();
    store.set(&parent_a, new_scope.clone()).unwrap();
    drop(store);
    let store = make();
    assert_eq!(store.take(&parent_a).unwrap(), Some(new_scope));
}
