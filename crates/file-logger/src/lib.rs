use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;

use domain::{ClassificationInput, ClassifierLogger, RedKind, Verdict};

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

    fn log_outcome(&self, input: &ClassificationInput, verdict: &Verdict) {
        let agent_id = &input.agent.id;
        match verdict {
            Verdict::Green => self.log_line(&format!("outcome: id={agent_id} verdict=Green")),
            Verdict::Red { kind, reason } => self.log_line(&format!(
                "outcome: id={agent_id} verdict=Red kind={} reason={reason:?}",
                red_kind_label(kind),
            )),
        }
    }

    fn log_error(&self, input: &ClassificationInput, error: &(dyn std::error::Error + 'static)) {
        let agent_id = &input.agent.id;
        let mut chain = error.to_string();
        let mut current = error.source();
        while let Some(c) = current {
            chain.push_str(": ");
            chain.push_str(&c.to_string());
            current = c.source();
        }
        self.log_line(&format!("error: id={agent_id} chain={chain:?}"));
    }
}

fn red_kind_label(kind: &RedKind) -> &'static str {
    match kind {
        RedKind::NoScopeRoot => "NoScopeRoot",
        RedKind::ScopeSetupCancelled => "ScopeSetupCancelled",
        RedKind::ScopeSetupUnavailable => "ScopeSetupUnavailable",
        RedKind::SubagentMissingScope => "SubagentMissingScope",
        RedKind::StoreUnavailable => "StoreUnavailable",
        RedKind::OutOfScope => "OutOfScope",
        RedKind::ClassifierUnavailable => "ClassifierUnavailable",
    }
}
