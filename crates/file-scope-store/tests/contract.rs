use std::fs;

use domain::test_support::agent_store_contract;
use domain::{AgentId, AgentStore, ScopeStoreError};
use file_scope_store::FileAgentStore;
use tempfile::tempdir;

#[test]
fn satisfies_agent_store_contract() {
    let dir = tempdir().unwrap();
    let path = dir.path().to_path_buf();
    agent_store_contract(|| FileAgentStore::new(path.clone()));
}

#[test]
fn corrupt_scope_file_is_unavailable_not_absent() {
    let dir = tempdir().unwrap();
    let store = FileAgentStore::new(dir.path().to_path_buf());
    let id = AgentId::try_new("sess").unwrap();
    let agent_dir = dir.path().join("sess");
    fs::create_dir_all(&agent_dir).unwrap();
    fs::write(agent_dir.join("scope.json"), "").unwrap();
    assert!(matches!(
        store.scope_for(&id),
        Err(ScopeStoreError::Unavailable(_))
    ));
}

#[test]
fn disable_creates_per_agent_dir_lazily() {
    let dir = tempdir().unwrap();
    let store = FileAgentStore::new(dir.path().to_path_buf());
    let id = AgentId::try_new("sess-2").unwrap();
    // Read on missing dir is `false`, not an error.
    assert!(!store.is_disabled(&id).unwrap());
    // Write creates the per-agent dir + the marker.
    store.disable(&id).unwrap();
    assert!(dir.path().join("sess-2").is_dir());
    assert!(dir.path().join("sess-2").join("disabled.marker").exists());
    assert!(store.is_disabled(&id).unwrap());
}

#[test]
fn clear_pending_failure_is_unavailable_not_silent() {
    let dir = tempdir().unwrap();
    let store = FileAgentStore::new(dir.path().to_path_buf());
    let id = AgentId::try_new("parent").unwrap();
    // A directory where the row should be: `remove_file` fails, row "remains".
    fs::create_dir_all(dir.path().join("parent").join("pending.json")).unwrap();
    assert!(matches!(
        store.clear_pending(&id),
        Err(ScopeStoreError::Unavailable(_))
    ));
}
