use std::path::PathBuf;

macro_rules! string_id {
    ($($name:ident),* $(,)?) => {
        $(
            #[derive(Clone, Debug, PartialEq, Eq, Hash)]
            pub struct $name(pub String);

            impl $name {
                pub fn as_str(&self) -> &str {
                    &self.0
                }
            }

            impl AsRef<str> for $name {
                fn as_ref(&self) -> &str {
                    &self.0
                }
            }

            impl std::fmt::Display for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{}", self.0)
                }
            }
        )*
    };
}

string_id!(SessionId, SpawnId, CallId, ActionKind, ActionArgs);

pub struct Session {
    pub id: SessionId,
    pub spawn_id: Option<SpawnId>,
    pub parent_id: Option<SessionId>,
}

pub struct Action {
    pub call_id: Option<CallId>,
    pub kind: ActionKind,
    pub args: ActionArgs,
}

pub struct ClassificationInput {
    pub session: Session,
    pub action: Action,
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
