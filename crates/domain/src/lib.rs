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

/// A scope-bearing identity. `id` is whatever uniquely identifies this agent for
/// scope purposes — for in-process subagents the adapter sets it to the
/// Claude-minted `agent_id`; for root and team-peer sessions it's the Claude
/// session id. `parent_id` (if any) is used by `resolve_scope` for inheritance.
pub struct Session {
    pub id: SessionId,
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

/// One-shot scope intent: parent records "the next child I spawn should run with this scope",
/// child reifies it on its own start hook (`SubagentStart` or `SessionStart`).
///
/// `take` reads and removes atomically (or as close as the backend allows) — once consumed,
/// the row is gone, so a second spawn without a fresh `set` won't accidentally reuse it.
/// `set` returns the previous value (if any) so callers can warn on overwrite.
pub trait PendingScopeStore {
    fn set(&self, parent: &SessionId, scope: Scope) -> Result<Option<Scope>, ScopeError>;
    fn take(&self, parent: &SessionId) -> Result<Option<Scope>, ScopeError>;
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

// Direct lookup, no parent walk. Each spawn must own its scope (set by the
// parent via `prepare_next_scope`); silent inheritance from parent would let
// narrowed children run under broader parent scopes — wrong direction. A
// subagent without its own scope is denied (RedKind::SubagentParentMissingScope)
// and the parent has to retry with a prepare.
pub fn resolve_scope<R: ScopeReader>(
    reader: &R,
    session: &Session,
) -> Result<Option<Scope>, ScopeError> {
    reader.scope_for(&session.id)
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

/// Records the parent's intent that the next agent it spawns should run with
/// this scope. Idempotent: a second call before the spawn overwrites — the
/// returned `Option<Scope>` carries the previous value so callers can warn.
pub fn prepare_next_scope<P: PendingScopeStore>(
    pending: &P,
    parent: &SessionId,
    scope: Scope,
) -> Result<Option<Scope>, ScopeError> {
    pending.set(parent, scope)
}

/// Consumes a pending child scope (if any) and writes it under the child's id.
/// Called from the child-start hook (`SubagentStart` or `SessionStart`). Returns
/// the materialised scope, or `Ok(None)` if no pending row existed (in which
/// case the child falls back to inheriting via `resolve_scope` at first tool call).
pub fn materialise_pending_scope<P, W>(
    pending: &P,
    scopes: &W,
    parent: &SessionId,
    child: &SessionId,
) -> Result<Option<Scope>, ScopeError>
where
    P: PendingScopeStore,
    W: ScopeWriter,
{
    let Some(scope) = pending.take(parent)? else {
        return Ok(None);
    };
    scopes.assign(child, scope.clone())?;
    Ok(Some(scope))
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
                reason: "subagent has no scope; parent must call `remit prepare-next-scope` before spawning".into(),
            },
            None => Verdict::Red {
                kind: RedKind::NoScopeRoot,
                reason: "no scope set for this session".into(),
            },
        }
    }
}
