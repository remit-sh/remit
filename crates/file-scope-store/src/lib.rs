use std::fs;
use std::io;
use std::path::PathBuf;

use domain::{AgentId, AgentStore, Scope, ScopeStoreError};

/// File-backed [`AgentStore`]. Single root directory; per-agent subdirectories
/// hold a small set of well-known files:
///
/// ```text
/// <agents_dir>/<agent-id>/
///   scope.json        — bound scope (any agent)
///   pending.json      — bundled scope staged for next child (parent only)
///   disabled.marker   — disable flag (root only; existence = disabled)
/// ```
///
/// `prompt.lock` lives in the same per-agent dir but is owned by `FsPromptLock`,
/// not by this store — see `prompt-lock` crate.
///
/// Per-agent dirs are created lazily on first write. Read errors that aren't
/// `NotFound` map to `ScopeStoreError::Unavailable` (fail-closed upstream).
pub struct FileAgentStore {
    agents_dir: PathBuf,
}

impl FileAgentStore {
    pub fn new(agents_dir: PathBuf) -> Self {
        Self { agents_dir }
    }

    fn agent_dir(&self, id: &AgentId) -> PathBuf {
        self.agents_dir.join(id.as_ref())
    }

    fn scope_path(&self, id: &AgentId) -> PathBuf {
        self.agent_dir(id).join("scope.json")
    }

    fn pending_path(&self, parent: &AgentId) -> PathBuf {
        self.agent_dir(parent).join("pending.json")
    }

    fn disabled_path(&self, id: &AgentId) -> PathBuf {
        self.agent_dir(id).join("disabled.marker")
    }
}

fn read_scope_json(path: &std::path::Path) -> Result<Option<Scope>, ScopeStoreError> {
    match fs::read_to_string(path) {
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

fn write_scope_json(path: &std::path::Path, scope: Scope) -> Result<(), ScopeStoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
    }
    let inner: String = scope.into_inner();
    let json =
        serde_json::to_string(&inner).map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
    fs::write(path, json).map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
    Ok(())
}

impl AgentStore for FileAgentStore {
    fn scope_for(&self, agent_id: &AgentId) -> Result<Option<Scope>, ScopeStoreError> {
        read_scope_json(&self.scope_path(agent_id))
    }

    fn assign_scope(&self, agent_id: &AgentId, scope: Scope) -> Result<(), ScopeStoreError> {
        write_scope_json(&self.scope_path(agent_id), scope)
    }

    fn set_pending(&self, parent: &AgentId, scope: Scope) -> Result<(), ScopeStoreError> {
        write_scope_json(&self.pending_path(parent), scope)
    }

    fn take_pending(&self, parent: &AgentId) -> Result<Option<Scope>, ScopeStoreError> {
        let path = self.pending_path(parent);
        let scope = read_scope_json(&path)?;
        if scope.is_some() {
            // Best-effort delete — if removal fails the row may be re-claimed
            // by a subsequent take, but the assigned scope under the child id
            // still wins downstream.
            let _ = fs::remove_file(&path);
        }
        Ok(scope)
    }

    fn is_disabled(&self, agent_id: &AgentId) -> Result<bool, ScopeStoreError> {
        match fs::metadata(self.disabled_path(agent_id)) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(ScopeStoreError::Unavailable(Box::new(e))),
        }
    }

    fn disable(&self, agent_id: &AgentId) -> Result<(), ScopeStoreError> {
        fs::create_dir_all(self.agent_dir(agent_id))
            .map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
        fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(self.disabled_path(agent_id))
            .map_err(|e| ScopeStoreError::Unavailable(Box::new(e)))?;
        Ok(())
    }
}
