use std::path::PathBuf;

use nutype::nutype;

#[nutype(derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef))]
pub struct SessionId(String);

#[nutype(derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef))]
pub struct SubagentId(String);

#[nutype(derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef))]
pub struct CallId(String);

#[nutype(derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef))]
pub struct ActionKind(String);

#[nutype(derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef))]
pub struct ActionArgs(String);

pub struct Action {
    pub call_id: Option<CallId>,
    pub kind: ActionKind,
    pub args: ActionArgs,
}

#[nutype(derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef))]
pub struct Scope(String);

pub struct Session {
    pub id: SessionId,
    pub subagent_id: Option<SubagentId>,
    pub parent_id: Option<SessionId>,
}

pub struct ClassificationInput {
    pub session: Session,
    pub action: Action,
    pub working_dir: PathBuf,
}

pub enum Verdict {
    Green,
    Red { reason: String },
}

pub trait ClassifierLogger {
    fn log_incoming(&self, input: &ClassificationInput);
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum ScopeStoreError {
    #[error("scope store backend unavailable")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub trait ScopeReader {
    fn scope_for(&self, session_id: &SessionId) -> Result<Option<Scope>, ScopeStoreError>;
}

pub trait ScopeWriter {
    fn assign(&self, session_id: &SessionId, scope: Scope) -> Result<(), ScopeStoreError>;
}

pub struct Classifier<L: ClassifierLogger> {
    logger: L,
}

impl<L: ClassifierLogger> Classifier<L> {
    pub fn new(logger: L) -> Self {
        Self { logger }
    }

    pub fn classify(&self, input: &ClassificationInput) -> Verdict {
        self.logger.log_incoming(input);
        Verdict::Green
    }
}
