use std::path::PathBuf;

pub struct Session {
    pub id: String,
    pub spawn_id: Option<String>,
}

pub struct ClassificationInput {
    pub session: Session,
    pub parent_session_id: Option<String>,
    pub call_id: Option<String>,
    pub action_kind: String,
    pub action: String,
    pub working_dir: PathBuf,
}

pub enum Decision {
    Green,
    Red { reason: String },
}

pub trait ClassifierLogger {
    fn log_incoming(&self, input: &ClassificationInput);
}

pub struct Classifier<L: ClassifierLogger> {
    logger: L,
}

impl<L: ClassifierLogger> Classifier<L> {
    pub fn new(logger: L) -> Self {
        Self { logger }
    }

    pub fn classify(&self, input: &ClassificationInput) -> Decision {
        self.logger.log_incoming(input);
        Decision::Green
    }
}
