use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;

use domain::{ClassificationInput, ClassifierLogger};

#[cfg(test)]
mod tests;

/// Append-only NDJSON logger. Implements [`ClassifierLogger`] and exposes
/// [`log_line`] for ad-hoc adapter messages (hook lifecycle events, staging, etc.).
/// Best-effort: IO errors are swallowed so a logging glitch never fails-open.
///
/// `markers`, when non-empty, are space-joined and prefixed to every line — used
/// by the composition root to surface flags whose state the audit trail should
/// reflect (e.g. `[dev-bypass-active]` when the firewall is disarmed).
///
/// [`log_line`]: FileLogger::log_line
#[derive(Clone)]
pub struct FileLogger {
    path: PathBuf,
    prefix: Option<String>,
}

impl FileLogger {
    pub fn new(path: PathBuf, markers: Vec<String>) -> Self {
        let prefix = (!markers.is_empty()).then(|| markers.join(" "));
        Self { path, prefix }
    }

    /// Append a single line to the log. Best-effort — IO errors are swallowed
    /// so the hook never fails-open due to a logging glitch.
    pub fn log_line(&self, line: &str) {
        if let Some(parent) = self.path.parent() {
            let _ = create_dir_all(parent);
        }
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        let _ = match &self.prefix {
            Some(prefix) => writeln!(file, "{prefix} {line}"),
            None => writeln!(file, "{line}"),
        };
    }
}

impl ClassifierLogger for FileLogger {
    fn log_incoming(&self, input: &ClassificationInput) {
        let parent_id = input.agent.parent_id.as_ref().map(|id| id.as_ref());
        let call_id = input.action.call_id.as_ref().map(|id| id.as_ref());
        self.log_line(&format!(
            "incoming: id={} parent_id={:?} call_id={:?} kind={} working_dir={} args={}",
            input.agent.id,
            parent_id,
            call_id,
            input.action.kind,
            input.working_dir.display(),
            input.action.args,
        ));
    }
}
