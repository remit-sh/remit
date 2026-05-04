use std::path::PathBuf;

use nutype::nutype;

/// Contract-test harness for adapters; opt-in via `test-support` feature so it never ships in prod.
#[cfg(feature = "test-support")]
pub mod test_support;

#[nutype(
    validate(predicate = |s: &str| !s.is_empty()
        && s.len() <= 128
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')),
    derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef)
)]
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
pub enum ScopeError {
    #[error("scope backend unavailable")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub trait ScopeReader {
    /// Fail-closed: consumers must treat `Ok(None)` and `Err(_)` as
    /// unknown state and emit Red. Adapters MUST return `Err(Unavailable)`
    /// for any non-absent failure (corrupt payload, IO error, permission
    /// denied, partial write); only confirmed absence returns `Ok(None)`.
    fn scope_for(&self, session_id: &SessionId) -> Result<Option<Scope>, ScopeError>;
}

pub trait ScopeWriter {
    fn assign(&self, session_id: &SessionId, scope: Scope) -> Result<(), ScopeError>;
}

// Single hop: walks session, then parent if present. The composition root
// flattens deeper chains via CLAUDE_SESSION_ID, which always points at root.
// Full chain-walk would need a SessionReader port; tracked separately.
pub fn resolve_scope<R: ScopeReader>(
    reader: &R,
    session: &Session,
) -> Result<Option<Scope>, ScopeError> {
    if let Some(scope) = reader.scope_for(&session.id)? {
        return Ok(Some(scope));
    }
    if let Some(parent) = &session.parent_id
        && parent != &session.id
    {
        return reader.scope_for(parent);
    }
    Ok(None)
}

pub struct Classifier<L: ClassifierLogger, R: ScopeReader> {
    logger: L,
    scope_reader: R,
}

impl<L: ClassifierLogger, R: ScopeReader> Classifier<L, R> {
    pub fn new(logger: L, scope_reader: R) -> Self {
        Self {
            logger,
            scope_reader,
        }
    }

    pub fn classify(&self, input: &ClassificationInput) -> Verdict {
        self.logger.log_incoming(input);
        match resolve_scope(&self.scope_reader, &input.session) {
            Ok(Some(_)) => Verdict::Green,
            Ok(None) => Verdict::Red {
                reason: "no scope set for this session".into(),
            },
            Err(e) => Verdict::Red {
                reason: format!("scope store unavailable: {e}"),
            },
        }
    }
}
