use std::fs;
use std::io;
use std::path::PathBuf;

use domain::{PendingScopeStore, Scope, ScopeError, ScopeReader, ScopeWriter, SessionId};

/// Active scopes: one `<id>.json` per scope. Same shape, two stores: `FileScopeStore`
/// owns the durable per-id rows; `FilePendingScopeStore` owns the parent's
/// "next-child intent" rows in a separate directory.
pub struct FileScopeStore {
    dir: PathBuf,
}

impl FileScopeStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path_for(&self, id: &SessionId) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

impl ScopeReader for FileScopeStore {
    fn scope_for(&self, session_id: &SessionId) -> Result<Option<Scope>, ScopeError> {
        read_scope_file(&self.path_for(session_id))
    }
}

impl ScopeWriter for FileScopeStore {
    fn assign(&self, session_id: &SessionId, scope: Scope) -> Result<(), ScopeError> {
        write_scope_file(&self.dir, &self.path_for(session_id), scope)
    }
}

/// Pending scopes: one `<parent-id>.json` per parent. Read-and-remove on `take`,
/// so a child's start hook consumes the row exactly once. Lives in a separate
/// dir from active scopes — they're different lifecycles, easier to reason
/// about and to introspect on disk.
pub struct FilePendingScopeStore {
    dir: PathBuf,
}

impl FilePendingScopeStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path_for(&self, id: &SessionId) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

impl PendingScopeStore for FilePendingScopeStore {
    fn set(&self, parent: &SessionId, scope: Scope) -> Result<Option<Scope>, ScopeError> {
        let path = self.path_for(parent);
        let prior = read_scope_file(&path)?;
        write_scope_file(&self.dir, &path, scope)?;
        Ok(prior)
    }

    fn take(&self, parent: &SessionId) -> Result<Option<Scope>, ScopeError> {
        let path = self.path_for(parent);
        let Some(scope) = read_scope_file(&path)? else {
            return Ok(None);
        };
        // Best-effort delete: if removal fails the next spawn may pick up a stale
        // intent, but the parent of that pending row is the same one we just
        // served, so the only failure mode is "same scope reused" — fail-open is
        // acceptable here, the alternative would be Red-on-clean-up which is
        // worse UX for a low-stakes hygiene step.
        let _ = fs::remove_file(&path);
        Ok(Some(scope))
    }
}

fn read_scope_file(path: &PathBuf) -> Result<Option<Scope>, ScopeError> {
    match fs::read_to_string(path) {
        Ok(content) => {
            let inner: String =
                serde_json::from_str(&content).map_err(|e| ScopeError::Unavailable(Box::new(e)))?;
            Ok(Some(Scope::new(inner)))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ScopeError::Unavailable(Box::new(e))),
    }
}

fn write_scope_file(dir: &PathBuf, path: &PathBuf, scope: Scope) -> Result<(), ScopeError> {
    fs::create_dir_all(dir).map_err(|e| ScopeError::Unavailable(Box::new(e)))?;
    let inner: String = scope.into_inner();
    let json = serde_json::to_string(&inner).map_err(|e| ScopeError::Unavailable(Box::new(e)))?;
    fs::write(path, json).map_err(|e| ScopeError::Unavailable(Box::new(e)))?;
    Ok(())
}
