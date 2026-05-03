use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;

use domain::{ClassificationInput, ClassifierLogger};

pub struct FileLogger {
    path: PathBuf,
}

impl FileLogger {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ClassifierLogger for FileLogger {
    fn log_incoming(&self, input: &ClassificationInput) {
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
        let spawn = input.session.spawn_id.as_ref().map(|s| s.as_str());
        let parent = input.session.parent_id.as_ref().map(|s| s.as_str());
        let call = input.action.call_id.as_ref().map(|s| s.as_str());
        let _ = writeln!(
            f,
            "incoming: session_id={} spawn_id={:?} parent_id={:?} call_id={:?} kind={} working_dir={} args={}",
            input.session.id,
            spawn,
            parent,
            call,
            input.action.kind,
            input.working_dir.display(),
            input.action.args,
        );
    }
}
