//! Filesystem-backed [`StandingGuidanceReader`]. Reads `REMIT.md` and per-Agent-type
//! templates from `.remit/agent-templates/<type>.md` from the discovered project
//! root. Walks up from the given `cwd` looking for each target; `.git` (file or
//! directory) acts as a project boundary that stops the walk before it strays
//! into ancestors.
//!
//! Houses every filesystem-backed contributor to [`StandingGuidance`]: project
//! guidance (TN-2026-008) and per-Agent-type templates (TN-2026-009).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use domain::{AgentType, StandingGuidance, StandingGuidanceError, StandingGuidanceReader};

#[cfg(test)]
mod tests;

/// Hard cap on any standing-guidance file's size. Defends every classification
/// (each file is read every call) against a pathological or hostile multi-MB
/// file. Exceeding the cap is an error, not a truncation — silent truncation
/// would break the "identical scope+action flips Red↔Green" contract.
pub const MAX_REMIT_BYTES: u64 = 64 * 1024;

const REMIT_FILENAME: &str = "REMIT.md";
const AGENT_TEMPLATES_DIR: &str = ".remit/agent-templates";
const GIT_DIRENT: &str = ".git";

/// Walk up from `cwd` looking for `target` (a path relative to each directory
/// visited). At each level: if `target` exists, return its absolute path;
/// otherwise if `.git` (file or directory) exists, stop with `Ok(None)`;
/// otherwise ascend. Stop at filesystem root.
///
/// `.git` acts as a project boundary, preventing accidental pickup of a stray
/// ancestor (e.g. `~/REMIT.md`) when the project itself has none.
///
/// Returns `Err` on IO errors during the walk (permission denied, symlink
/// loop, etc.) — fail-closed: callers map this to a Red verdict rather than
/// treating the error as "absence" and ascending past it.
fn walk_up_find(cwd: &Path, target: &Path) -> Result<Option<PathBuf>, io::Error> {
    debug_assert!(
        cwd.is_absolute(),
        "walk_up_find expects an absolute cwd; got {cwd:?}"
    );
    let mut current = cwd;
    loop {
        let candidate = current.join(target);
        if candidate.try_exists()? {
            return Ok(Some(candidate));
        }
        if current.join(GIT_DIRENT).try_exists()? {
            return Ok(None);
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return Ok(None),
        }
    }
}

/// Walk-up discovery for `REMIT.md`. Thin wrapper over [`walk_up_find`].
///
/// Adapter-specific helper. Hosts that need the path for structural defence
/// (e.g. write-protection on `REMIT.md`) call this directly.
pub fn discover_remit_path(cwd: &Path) -> Result<Option<PathBuf>, io::Error> {
    walk_up_find(cwd, Path::new(REMIT_FILENAME))
}

/// Walk-up discovery for an agent-template path. Same `.git`-boundary semantics
/// as [`discover_remit_path`]. Returns `Ok(None)` when no
/// `.remit/agent-templates/<type>.md` is found before the boundary; `Err`
/// propagates IO errors fail-closed.
pub fn discover_agent_template_path(
    cwd: &Path,
    agent_type: &AgentType,
) -> Result<Option<PathBuf>, io::Error> {
    let target = PathBuf::from(AGENT_TEMPLATES_DIR).join(format!("{}.md", agent_type.as_ref()));
    walk_up_find(cwd, &target)
}

pub struct FileStandingGuidanceReader {
    cwd: PathBuf,
    agent_type: Option<AgentType>,
}

impl FileStandingGuidanceReader {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            agent_type: None,
        }
    }

    /// Attach an [`AgentType`] for the agent-template slot. `None` leaves the
    /// slot empty regardless of filesystem state. Builder pattern keeps the
    /// existing `new(cwd)` call sites unchanged.
    pub fn with_agent_type(mut self, agent_type: Option<AgentType>) -> Self {
        self.agent_type = agent_type;
        self
    }

    fn read_project_remit(&self) -> Result<Option<String>, StandingGuidanceError> {
        let path = match discover_remit_path(&self.cwd) {
            Ok(Some(p)) => p,
            Ok(None) => return Ok(None),
            Err(e) => {
                return Err(StandingGuidanceError::Unavailable(Box::new(e)));
            }
        };
        read_capped_utf8(&path, "REMIT.md").map(Some)
    }

    fn read_agent_template(&self) -> Result<Option<String>, StandingGuidanceError> {
        let Some(agent_type) = self.agent_type.as_ref() else {
            return Ok(None);
        };
        let path = match discover_agent_template_path(&self.cwd, agent_type) {
            Ok(Some(p)) => p,
            Ok(None) => return Ok(None),
            Err(e) => {
                return Err(StandingGuidanceError::Unavailable(Box::new(e)));
            }
        };
        read_capped_utf8(&path, "agent template").map(Some)
    }
}

impl StandingGuidanceReader for FileStandingGuidanceReader {
    fn read(&self) -> Result<StandingGuidance, StandingGuidanceError> {
        let project = self.read_project_remit()?;
        let agent_template = self.read_agent_template()?;
        Ok(StandingGuidance {
            project,
            agent_template,
        })
    }
}

/// Strict UTF-8 read with 64 KB cap and metadata-checked file type. `label` is
/// the human-friendly file name used in error messages (e.g. `"REMIT.md"`,
/// `"agent template"`). Caller already discovered the path; this performs the
/// uniform fail-closed read for every standing-guidance contributor.
fn read_capped_utf8(path: &Path, label: &str) -> Result<String, StandingGuidanceError> {
    let metadata =
        fs::metadata(path).map_err(|e| StandingGuidanceError::Unavailable(Box::new(e)))?;

    if !metadata.is_file() {
        return Err(StandingGuidanceError::Unavailable(
            format!("{label} is not a regular file").into(),
        ));
    }

    if metadata.len() > MAX_REMIT_BYTES {
        return Err(StandingGuidanceError::Unavailable(
            format!(
                "{label} exceeds {MAX_REMIT_BYTES} bytes ({} bytes)",
                metadata.len()
            )
            .into(),
        ));
    }

    // Size cap is enforced before reading bytes: the metadata-only check
    // bounds memory allocation. fs::read after the cap reads at most 64 KB.
    let bytes = fs::read(path).map_err(|e| StandingGuidanceError::Unavailable(Box::new(e)))?;

    let text = String::from_utf8(bytes).map_err(|_| {
        StandingGuidanceError::Unavailable(format!("{label} is not valid UTF-8").into())
    })?;

    Ok(strip_bom(text))
}

fn strip_bom(s: String) -> String {
    const BOM: &str = "\u{FEFF}";
    if let Some(stripped) = s.strip_prefix(BOM) {
        stripped.to_string()
    } else {
        s
    }
}
