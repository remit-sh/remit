use std::fs::{File, OpenOptions, create_dir_all};
use std::path::PathBuf;

use domain::{AgentId, PromptError, PromptLock};

/// Filesystem-backed [`PromptLock`]. Per-agent lock file under
/// `<agents_dir>/<agent-id>/prompt.lock` — same `agents_dir` the
/// `FileAgentStore` uses, so all agent-keyed state lives under one tree.
/// `acquire` blocks behind any in-flight owner of the same agent's lock; the
/// returned [`FsPromptGuard`] releases on drop.
///
/// Pattern mirrors `FileLogger::log_line` (TN-2026-006): standard library
/// [`File::lock`] for cross-process exclusive locking. Different agents hash
/// to different files, so they never block each other.
pub struct FsPromptLock {
    agents_dir: PathBuf,
}

impl FsPromptLock {
    pub fn new(agents_dir: PathBuf) -> Self {
        Self { agents_dir }
    }

    fn lock_path(&self, agent_id: &AgentId) -> PathBuf {
        self.agents_dir.join(agent_id.as_ref()).join("prompt.lock")
    }
}

/// RAII guard. Holding it means we own the per-agent prompt lock; dropping it
/// releases (the kernel does so when the underlying [`File`] is closed).
pub struct FsPromptGuard {
    _file: File,
}

impl PromptLock for FsPromptLock {
    type Guard = FsPromptGuard;

    fn acquire(&self, agent_id: &AgentId) -> Result<Self::Guard, PromptError> {
        let path = self.lock_path(agent_id);
        if let Some(parent) = path.parent() {
            create_dir_all(parent).map_err(|e| PromptError::Unavailable(Box::new(e)))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| PromptError::Unavailable(Box::new(e)))?;
        // Blocking exclusive lock — queues behind any in-flight owner of the
        // same agent's lock. Cross-process correctness via the kernel.
        file.lock()
            .map_err(|e| PromptError::Unavailable(Box::new(e)))?;
        Ok(FsPromptGuard { _file: file })
    }
}
