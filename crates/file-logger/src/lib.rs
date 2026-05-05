use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;

use domain::{ClassificationInput, ClassifierLogger};

#[derive(Clone)]
pub struct FileLogger {
    path: PathBuf,
}

impl FileLogger {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Append a free-form line. Used by the hook adapter to log non-classification
    /// events (prepare, materialise, lookup) alongside the classifier-driven
    /// `incoming:` lines, so the whole flow is visible in one log.
    pub fn log_line(&self, line: &str) {
        self.append(line);
    }

    fn append(&self, line: &str) {
        if let Some(parent) = self.path.parent() {
            let _ = create_dir_all(parent);
        }
        let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        let _ = writeln!(f, "{line}");
    }
}

impl ClassifierLogger for FileLogger {
    fn log_incoming(&self, input: &ClassificationInput) {
        let parent = input.session.parent_id.as_ref().map(|s| s.as_ref());
        let call = input.action.call_id.as_ref().map(|s| s.as_ref());
        self.append(&format!(
            "incoming: id={} parent_id={:?} call_id={:?} kind={} working_dir={} args={}",
            input.session.id,
            parent,
            call,
            input.action.kind,
            input.working_dir.display(),
            input.action.args,
        ));
    }
}
