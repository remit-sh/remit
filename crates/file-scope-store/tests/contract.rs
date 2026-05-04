use std::fs;

use domain::test_support::scope_store_contract;
use domain::{ScopeError, ScopeReader, SessionId};
use file_scope_store::FileScopeStore;
use tempfile::tempdir;

#[test]
fn satisfies_scope_store_contract() {
    let dir = tempdir().unwrap();
    let path = dir.path().to_path_buf();
    scope_store_contract(|| FileScopeStore::new(path.clone()));
}

#[test]
fn corrupt_file_is_unavailable_not_absent() {
    let dir = tempdir().unwrap();
    let store = FileScopeStore::new(dir.path().to_path_buf());
    let id = SessionId::try_new("sess").unwrap();
    fs::write(dir.path().join("sess.json"), "").unwrap();
    assert!(matches!(
        store.scope_for(&id),
        Err(ScopeError::Unavailable(_))
    ));
}
