use std::path::PathBuf;

pub struct ClassificationInput {
    pub session_id: String,
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

pub fn classify(input: &ClassificationInput, logger: &dyn ClassifierLogger) -> Decision {
    logger.log_incoming(input);
    Decision::Green
}
