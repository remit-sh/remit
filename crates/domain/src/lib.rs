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

impl Session {
    pub fn has_parent(&self) -> bool {
        self.parent_id.as_ref().is_some_and(|p| p != &self.id)
    }
}

pub struct ClassificationInput {
    pub session: Session,
    pub action: Action,
    pub working_dir: PathBuf,
}

pub enum Verdict {
    Green,
    Red { kind: RedKind, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RedKind {
    /// Root session, no scope present, no prompt has been attempted yet.
    NoScopeRoot,
    /// User dismissed the scope prompt.
    ScopeSetupCancelled,
    /// Prompter failed (e.g. osascript missing, IO error).
    ScopeSetupUnavailable,
    /// Subagent's parent walk found no scope; can't prompt mid-subagent.
    SubagentParentMissingScope,
    /// Scope reader or writer reported an error.
    StoreUnavailable,
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

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum PromptError {
    #[error("prompt cancelled by user")]
    Cancelled,
    #[error("prompt unavailable")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub trait ScopePrompter {
    /// Asks the user for a scope. Cancellation is a normal outcome, not an error
    /// to the same degree as `Unavailable` — the composition root maps each to
    /// a distinct verdict reason.
    fn prompt(&self, session_id: &SessionId) -> Result<Scope, PromptError>;
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum AcquireScopeError {
    #[error("scope store: {0}")]
    Store(#[from] ScopeError),
    #[error("prompt: {0}")]
    Prompt(#[from] PromptError),
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
    if session.has_parent() {
        return reader.scope_for(session.parent_id.as_ref().unwrap());
    }
    Ok(None)
}

/// Resolves the session's scope, prompting and persisting if absent on a root session.
/// Subagents (sessions with a parent) never trigger a prompt — they must inherit.
pub fn acquire_scope<R, P, W>(
    reader: &R,
    prompter: &P,
    writer: &W,
    session: &Session,
) -> Result<Option<Scope>, AcquireScopeError>
where
    R: ScopeReader,
    P: ScopePrompter,
    W: ScopeWriter,
{
    if let Some(scope) = resolve_scope(reader, session)? {
        return Ok(Some(scope));
    }
    if session.has_parent() {
        return Ok(None);
    }
    match prompter.prompt(&session.id) {
        Ok(scope) => {
            writer.assign(&session.id, scope.clone())?;
            Ok(Some(scope))
        }
        Err(PromptError::Cancelled) => Ok(None),
        Err(e @ PromptError::Unavailable(_)) => Err(AcquireScopeError::Prompt(e)),
    }
}

/// One end-to-end host-agnostic step: acquire the session's scope (prompting on a
/// root with no scope), then classify the action. The composition root in any host
/// adapter calls this and only translates the resulting `Verdict` into its own
/// transport (exit codes, JSON, etc.).
pub fn process_action<R, P, W, L>(
    input: &ClassificationInput,
    reader: &R,
    prompter: &P,
    writer: &W,
    classifier: &Classifier<L>,
) -> Verdict
where
    R: ScopeReader,
    P: ScopePrompter,
    W: ScopeWriter,
    L: ClassifierLogger,
{
    match acquire_scope(reader, prompter, writer, &input.session) {
        Err(AcquireScopeError::Prompt(e)) => Verdict::Red {
            kind: RedKind::ScopeSetupUnavailable,
            reason: format!("scope prompt unavailable: {e}"),
        },
        Err(AcquireScopeError::Store(e)) => Verdict::Red {
            kind: RedKind::StoreUnavailable,
            reason: format!("scope store unavailable: {e}"),
        },
        Ok(scope) => match classifier.classify(input, scope.as_ref()) {
            Verdict::Red {
                kind: RedKind::NoScopeRoot,
                ..
            } => Verdict::Red {
                kind: RedKind::ScopeSetupCancelled,
                reason: "scope setup cancelled; nothing was set".into(),
            },
            v => v,
        },
    }
}

pub struct Classifier<L: ClassifierLogger> {
    logger: L,
}

impl<L: ClassifierLogger> Classifier<L> {
    pub fn new(logger: L) -> Self {
        Self { logger }
    }

    pub fn classify(&self, input: &ClassificationInput, scope: Option<&Scope>) -> Verdict {
        self.logger.log_incoming(input);
        match scope {
            Some(_) => Verdict::Green,
            None if input.session.has_parent() => Verdict::Red {
                kind: RedKind::SubagentParentMissingScope,
                reason: "subagent has no inherited scope; set scope on the parent session".into(),
            },
            None => Verdict::Red {
                kind: RedKind::NoScopeRoot,
                reason: "no scope set for this session".into(),
            },
        }
    }
}
