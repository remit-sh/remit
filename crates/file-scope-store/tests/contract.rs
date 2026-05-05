use std::fs;

use domain::test_support::{pending_scope_store_contract, scope_store_contract};
use domain::{AgentId, ScopeReader, ScopeStoreError};
use file_scope_store::{FilePendingScopeStore, FileScopeStore};
use tempfile::tempdir;

#[test]
fn satisfies_scope_store_contract() {
    let dir = tempdir().unwrap();
    let path = dir.path().to_path_buf();
    scope_store_contract(|| FileScopeStore::new(path.clone()));
}

#[test]
fn corrupt_scope_file_is_unavailable_not_absent() {
    let dir = tempdir().unwrap();
    let store = FileScopeStore::new(dir.path().to_path_buf());
    let id = AgentId::try_new("sess").unwrap();
    fs::write(dir.path().join("sess.json"), "").unwrap();
    assert!(matches!(
        store.scope_for(&id),
        Err(ScopeStoreError::Unavailable(_))
    ));
}

#[test]
fn satisfies_pending_scope_store_contract() {
    let dir = tempdir().unwrap();
    let path = dir.path().to_path_buf();
    pending_scope_store_contract(|| FilePendingScopeStore::new(path.clone()));
}
