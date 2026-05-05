use std::path::PathBuf;

use nutype::nutype;

/// Contract-test harness for adapters; opt-in via `test-support` feature so it never ships in prod.
#[cfg(feature = "test-support")]
pub mod test_support;

/// Identity of an [`Agent`]. Adapter-collapsed: a Claude Code in-process
/// subagent's `agent_id` and a session's `session_id` both flatten into this.
#[nutype(
    validate(predicate = |s: &str| !s.is_empty()
        && s.len() <= 128
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')),
    derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef)
)]
pub struct AgentId(String);

/// Runtime tool-call id (e.g. Claude Code's `tool_use_id`). Adapter-defined; opaque to the domain.
#[nutype(derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef))]
pub struct CallId(String);

#[nutype(derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef))]
pub struct ActionKind(String);

#[nutype(derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef))]
pub struct ActionArgs(String);

/// A tool call to be classified.
/// Vocab: [Action](../../../docs/UBIQUITOUS_LANGUAGE.md#classification).
pub struct Action {
    pub call_id: Option<CallId>,
    pub kind: ActionKind,
    pub args: ActionArgs,
}

/// The agent's task — what it's trying to accomplish. Not an allowlist.
/// Empty / whitespace-only is rejected; fail-closed at the boundary.
/// Vocab: [Scope](../../../docs/UBIQUITOUS_LANGUAGE.md#product).
#[nutype(
    validate(predicate = |s: &str| !s.trim().is_empty()),
    derive(Clone, Debug, Display, PartialEq, Eq, Hash, AsRef)
)]
pub struct Scope(String);

/// A scope-bearing runtime instance. `parent_id` is the spawner's identity,
/// used by adapters to look up pending rows on a child's start hook.
/// Vocab: [Agent](../../../docs/UBIQUITOUS_LANGUAGE.md#agents).
pub struct Agent {
    pub id: AgentId,
    pub parent_id: Option<AgentId>,
}

impl Agent {
    /// Top-level Agent — no parent. Scope is acquired via prompter.
    pub fn root(id: AgentId) -> Self {
        Self {
            id,
            parent_id: None,
        }
    }

    /// Child Agent under a parent. Self-referential parent (id == parent)
    /// collapses to root — defends against malformed env / payload state.
    /// Vocab: [Subagent](../../../docs/UBIQUITOUS_LANGUAGE.md#agents).
    pub fn subagent(id: AgentId, parent: AgentId) -> Self {
        if id == parent {
            Self::root(id)
        } else {
            Self {
                id,
                parent_id: Some(parent),
            }
        }
    }

    pub fn has_parent(&self) -> bool {
        self.parent_id.is_some()
    }
}

/// Agent + Action + working dir — the unit the [`Classifier`] turns into a [`Verdict`].
/// Vocab: [Classification input](../../../docs/UBIQUITOUS_LANGUAGE.md#classification).
pub struct ClassificationInput {
    pub agent: Agent,
    pub action: Action,
    pub working_dir: PathBuf,
}

/// Classifier output. Green = action serves the [`Scope`]. Red = it doesn't,
/// and `reason` says why. Only Red carries a [`RedKind`] + reason.
/// Vocab: [Verdict](../../../docs/UBIQUITOUS_LANGUAGE.md#classification).
pub enum Verdict {
    Green,
    Red { kind: RedKind, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RedKind {
    /// Root agent, no scope present, no prompt has been attempted yet.
    NoScopeRoot,
    /// User dismissed the scope prompt.
    ScopeSetupCancelled,
    /// Prompter failed (e.g. osascript missing, IO error).
    ScopeSetupUnavailable,
    /// Subagent reached classification with no bound scope — parent's spawn payload
    /// was missing the bundled scope, or the staging step failed before the child started.
    SubagentMissingScope,
    /// Scope reader or writer reported an error.
    StoreUnavailable,
}

/// Outbound observability port — the [`Classifier`] hands every classification
/// here for adapters to log / surface as they like.
pub trait ClassifierLogger {
    fn log_incoming(&self, input: &ClassificationInput);
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum ScopeStoreError {
    #[error("scope backend unavailable")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Read a bound [`Scope`] for an [`Agent`].
/// Fail-closed: `Ok(None)` and `Err(_)` both mean "no usable scope" — emit Red.
/// Adapters MUST return `Err(Unavailable)` for any non-absent failure
/// (corrupt payload, IO error, permission denied); only confirmed absence is `Ok(None)`.
pub trait ScopeReader {
    fn scope_for(&self, agent_id: &AgentId) -> Result<Option<Scope>, ScopeStoreError>;
}

/// Bind a [`Scope`] to an [`Agent`].
pub trait ScopeWriter {
    fn assign(&self, agent_id: &AgentId, scope: Scope) -> Result<(), ScopeStoreError>;
}

/// Parent-keyed staging slot for the **bundled scope** of the next child the
/// parent spawns. Lifecycle: parent's spawn-tool `PreToolUse` → [`set`] →
/// child's start hook → [`take`] → [`ScopeWriter::assign`] under the child's id.
/// Single-slot per parent in MVP; FIFO tracked as #20.
/// Vocab: [Bundled scope](../../../docs/UBIQUITOUS_LANGUAGE.md#agents).
///
/// [`set`]: PendingScopeStore::set
/// [`take`]: PendingScopeStore::take
pub trait PendingScopeStore {
    fn set(&self, parent: &AgentId, scope: Scope) -> Result<(), ScopeStoreError>;
    fn take(&self, parent: &AgentId) -> Result<Option<Scope>, ScopeStoreError>;
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum PromptError {
    #[error("prompt cancelled by user")]
    Cancelled,
    #[error("prompt unavailable")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Ask the user for a [`Scope`] — root sessions only. `Cancelled` is a normal
/// outcome (distinct from `Unavailable`); composition root maps each to its own [`RedKind`].
pub trait ScopePrompter {
    fn prompt(&self, agent_id: &AgentId) -> Result<Scope, PromptError>;
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum AcquireScopeError {
    #[error("scope store: {0}")]
    Store(#[from] ScopeStoreError),
    #[error("prompt: {0}")]
    Prompt(#[from] PromptError),
}

/// Look up the [`Scope`] bound to this agent's id.
pub fn resolve_scope<R: ScopeReader>(
    reader: &R,
    agent: &Agent,
) -> Result<Option<Scope>, ScopeStoreError> {
    reader.scope_for(&agent.id)
}

/// Resolve the agent's scope, prompting + persisting if absent on a root.
/// Subagents return `Ok(None)` rather than prompting — their scope must already
/// have been materialised by the start-event hook.
pub fn acquire_scope<R, P, W>(
    reader: &R,
    prompter: &P,
    writer: &W,
    agent: &Agent,
) -> Result<Option<Scope>, AcquireScopeError>
where
    R: ScopeReader,
    P: ScopePrompter,
    W: ScopeWriter,
{
    if let Some(scope) = resolve_scope(reader, agent)? {
        return Ok(Some(scope));
    }
    if agent.has_parent() {
        return Ok(None);
    }
    match prompter.prompt(&agent.id) {
        Ok(scope) => {
            writer.assign(&agent.id, scope.clone())?;
            Ok(Some(scope))
        }
        Err(PromptError::Cancelled) => Ok(None),
        Err(e @ PromptError::Unavailable(_)) => Err(AcquireScopeError::Prompt(e)),
    }
}

/// One host-agnostic step: acquire the agent's scope, then classify the action.
/// Composition roots call this and translate the resulting [`Verdict`] into their
/// own transport (exit codes, JSON, etc.).
pub fn process_action<R, P, W, L>(
    input: &ClassificationInput,
    scope_reader: &R,
    prompter: &P,
    scope_writer: &W,
    classifier: &Classifier<L>,
) -> Verdict
where
    R: ScopeReader,
    P: ScopePrompter,
    W: ScopeWriter,
    L: ClassifierLogger,
{
    match acquire_scope(scope_reader, prompter, scope_writer, &input.agent) {
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

/// Maps one [`ClassificationInput`] to one [`Verdict`]. Pure: presence of scope → Green;
/// absence → Red with the appropriate [`RedKind`]. The semantic firewall.
/// Vocab: [Classifier](../../../docs/UBIQUITOUS_LANGUAGE.md#classification).
pub struct Classifier<L: ClassifierLogger> {
    logger: L,
}

impl<L: ClassifierLogger> Classifier<L> {
    pub fn new(logger: L) -> Self {
        Self { logger }
    }

    /// Classify one action. Logs the incoming input via [`ClassifierLogger`] before
    /// emitting the verdict. Always returns either Green or a Red with a typed reason.
    pub fn classify(&self, input: &ClassificationInput, scope: Option<&Scope>) -> Verdict {
        self.logger.log_incoming(input);
        match scope {
            Some(_) => Verdict::Green,
            None if input.agent.has_parent() => Verdict::Red {
                kind: RedKind::SubagentMissingScope,
                reason: "subagent has no scope; parent's spawn payload missing or staging failed"
                    .into(),
            },
            None => Verdict::Red {
                kind: RedKind::NoScopeRoot,
                reason: "no scope set for this agent".into(),
            },
        }
    }
}
