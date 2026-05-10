//! Filesystem-backed [`StandingGuidanceReader`]. Reads `REMIT.md` from the
//! discovered project root. Walks up from the given `cwd` looking for
//! `REMIT.md`; `.git` (file or directory) acts as a project boundary that stops
//! the walk before it strays into ancestors.
//!
//! Houses every filesystem-backed contributor to [`StandingGuidance`]. Today
//! that is just `REMIT.md` (TN-2026-008); TN-2026-009 (#32) will add
//! per-Agent-type templates from `.remit/agent-templates/<type>.md` to the same
//! adapter.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use domain::{StandingGuidance, StandingGuidanceError, StandingGuidanceReader};

#[cfg(test)]
mod tests;

/// Hard cap on `REMIT.md` size. Defends every classification (the file is read
/// every call) against a pathological or hostile multi-MB file. Exceeding the
/// cap is an error, not a truncation — silent truncation would break the
/// "identical scope+action flips Red↔Green" contract.
pub const MAX_REMIT_BYTES: u64 = 64 * 1024;

const REMIT_FILENAME: &str = "REMIT.md";
const GIT_DIRENT: &str = ".git";

/// Walk up from `cwd` looking for `REMIT.md`. At each level: if `REMIT.md`
/// exists, return its path; otherwise if `.git` (file or directory) exists,
/// stop with `Ok(None)`; otherwise ascend. Stop at filesystem root.
///
/// `.git` acts as a project boundary, preventing accidental pickup of a stray
/// ancestor `REMIT.md` (e.g. `~/REMIT.md`) when the project itself has none.
///
/// Returns `Err` on IO errors during the walk (permission denied on an ancestor,
/// symlink loop, etc.) — fail-closed: callers map this to a Red verdict rather
/// than treating the error as "absence" and ascending past it.
///
/// Adapter-specific helper. Hosts that need the path for structural defence
/// (e.g. write-protection on `REMIT.md`) call this directly.
pub fn discover_remit_path(cwd: &Path) -> Result<Option<PathBuf>, io::Error> {
    debug_assert!(
        cwd.is_absolute(),
        "discover_remit_path expects an absolute cwd; got {cwd:?}"
    );
    let mut current = cwd;
    loop {
        let remit = current.join(REMIT_FILENAME);
        if remit.try_exists()? {
            return Ok(Some(remit));
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

pub struct FileStandingGuidanceReader {
    cwd: PathBuf,
}

impl FileStandingGuidanceReader {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    fn read_project_remit(&self) -> Result<Option<String>, StandingGuidanceError> {
        let path = match discover_remit_path(&self.cwd) {
            Ok(Some(p)) => p,
            Ok(None) => return Ok(None),
            Err(e) => {
                return Err(StandingGuidanceError::Unavailable(Box::new(e)));
            }
        };

        let metadata =
            fs::metadata(&path).map_err(|e| StandingGuidanceError::Unavailable(Box::new(e)))?;

        if !metadata.is_file() {
            return Err(StandingGuidanceError::Unavailable(
                "REMIT.md is not a regular file".into(),
            ));
        }

        if metadata.len() > MAX_REMIT_BYTES {
            return Err(StandingGuidanceError::Unavailable(
                format!(
                    "REMIT.md exceeds {MAX_REMIT_BYTES} bytes ({} bytes)",
                    metadata.len()
                )
                .into(),
            ));
        }

        // Size cap is enforced before reading bytes: the metadata-only check
        // bounds memory allocation. fs::read after the cap reads at most 64 KB.
        let bytes = fs::read(&path).map_err(|e| StandingGuidanceError::Unavailable(Box::new(e)))?;

        let text = String::from_utf8(bytes).map_err(|_| {
            StandingGuidanceError::Unavailable("REMIT.md is not valid UTF-8".into())
        })?;

        Ok(Some(strip_bom(text)))
    }
}

impl StandingGuidanceReader for FileStandingGuidanceReader {
    fn read(&self) -> Result<StandingGuidance, StandingGuidanceError> {
        let project = self.read_project_remit()?;
        Ok(StandingGuidance {
            project,
            // Reserved for TN-2026-009 (#32). The umbrella port carries the slot
            // so the public surface is stable across the two changes.
            agent_template: None,
        })
    }
}

fn strip_bom(s: String) -> String {
    const BOM: &str = "\u{FEFF}";
    if let Some(stripped) = s.strip_prefix(BOM) {
        stripped.to_string()
    } else {
        s
    }
}
