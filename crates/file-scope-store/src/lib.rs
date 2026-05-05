use std::fs;
use std::io;
use std::path::PathBuf;

use domain::{AgentId, PendingScopeStore, Scope, ScopeReader, ScopeStoreError, ScopeWriter};

/// File-backed bound-scope store: one JSON file per agent id under `dir`.
/// One row per agent; `assign` overwrites in place.
pub struct FileScopeStore {
    dir: PathBuf,
}

impl FileScopeStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path_for(&self, id: &AgentId) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

impl ScopeReader for FileScopeStore {
    fn scope_for(&self, agent_id: &AgentId) -> Result<Option<Scope>, ScopeStoreError> {
        match fs::read_to_string(self.path_for(agent_id)) {
            Ok(content) => {
                let inner: String = serde_json::from_str(&content)
                    .map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
                let scope =
                    Scope::try_new(inner).map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
                Ok(Some(scope))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ScopeStoreError::Unavailable(Box::new(e))),
        }
    }
}

impl ScopeWriter for FileScopeStore {
    fn assign(&self, agent_id: &AgentId, scope: Scope) -> Result<(), ScopeStoreError> {
        fs::create_dir_all(&self.dir).map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
        let inner: String = scope.into_inner();
        let json =
            serde_json::to_string(&inner).map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
        fs::write(self.path_for(agent_id), json)
            .map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
        Ok(())
    }
}

/// File-backed pending-scope staging: same JSON-per-key shape as [`FileScopeStore`],
/// but `take` deletes after reading. Single-slot per parent in MVP (#20 tracks FIFO).
pub struct FilePendingScopeStore {
    dir: PathBuf,
}

impl FilePendingScopeStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path_for(&self, parent: &AgentId) -> PathBuf {
        self.dir.join(format!("{parent}.json"))
    }
}

impl PendingScopeStore for FilePendingScopeStore {
    fn set(&self, parent: &AgentId, scope: Scope) -> Result<(), ScopeStoreError> {
        fs::create_dir_all(&self.dir).map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
        let inner: String = scope.into_inner();
        let json =
            serde_json::to_string(&inner).map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
        fs::write(self.path_for(parent), json)
            .map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
        Ok(())
    }

    fn take(&self, parent: &AgentId) -> Result<Option<Scope>, ScopeStoreError> {
        let path = self.path_for(parent);
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ScopeStoreError::Unavailable(Box::new(e))),
        };
        let inner: String = serde_json::from_str(&content)
            .map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
        let scope = Scope::try_new(inner).map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
        // Best-effort delete — if removal fails the row may be re-claimed by a
        // subsequent take, but the assigned scope under the child id still wins.
        let _ = fs::remove_file(&path);
        Ok(Some(scope))
    }
}
