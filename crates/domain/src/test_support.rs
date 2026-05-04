use crate::{Scope, ScopeReader, ScopeWriter, SessionId};

/// Contract test for `ScopeReader` + `ScopeWriter` implementations.
/// Adapters call this from a `#[test]` to prove conformance — same suite for every backend.
/// Takes a factory closure so persistence can be tested across fresh instances.
pub fn scope_store_contract<S, F>(make: F)
where
    S: ScopeReader + ScopeWriter,
    F: Fn() -> S,
{
    let id_a = SessionId::try_new("contract-test-session-a").unwrap();
    let id_b = SessionId::try_new("contract-test-session-b").unwrap();
    let scope_1 = Scope::new("first scope");
    let scope_2 = Scope::new("second scope");
    let long = Scope::new("x".repeat(64 * 1024));
    let unicode = Scope::new("范围 — naïve café 🦀");

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
