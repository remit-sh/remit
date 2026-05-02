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
        let _ = writeln!(
            f,
            "incoming: session_id={} spawn_id={:?} parent_id={:?} call_id={:?} kind={} working_dir={} args={}",
            input.session.id,
            input.session.spawn_id,
            input.session.parent_id,
            input.action.call_id,
            input.action.kind,
            input.working_dir.display(),
            input.action.args,
        );
    }
}
