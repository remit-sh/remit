// Each integration-test binary in `tests/` only uses a subset of these helpers;
// allow per-binary dead code rather than splitting fakes across files.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::rc::Rc;

use domain::{
    Action, AgentId, AgentStore, ClassificationContext, ClassificationHistoryWriter,
    ClassifierOutcome, LlmCallStats, LlmClassifier, LlmClassifierError, PromptError, PromptLock,
    Prompter, RawVerdict, RedKind, Scope, ScopeStoreError, StandingGuidance, StandingGuidanceError,
    StandingGuidanceReader, Verdict, VerdictChoice,
};

/// Fake [`AgentStore`] for domain tests. Holds in-memory state for scope, pending,
/// and disable; supports failure-injection knobs per concern.
///
/// Build via `FakeAgentStore::new()` and chain `.with_scope(id, scope)` /
/// `.with_disable_states(...)` / etc. Failure modes are scoped (read-fail,
/// write-fail, disable-read-fail) so tests can target specific paths.
pub struct FakeAgentStore {
    scopes: Rc<RefCell<HashMap<AgentId, Scope>>>,
    scope_writes: Rc<RefCell<Vec<(AgentId, Scope)>>>,
    pending: Rc<RefCell<HashMap<AgentId, Scope>>>,
    disabled: Rc<RefCell<HashMap<AgentId, ()>>>,
    disable_writes: Rc<RefCell<Vec<AgentId>>>,
    fail_scope_read: Rc<RefCell<bool>>,
    fail_scope_write: Rc<RefCell<bool>>,
    fail_pending_clear: Rc<RefCell<bool>>,
    fail_pending_write: Rc<RefCell<bool>>,
    fail_disable_read: Rc<RefCell<bool>>,
    fail_disable_write: Rc<RefCell<bool>>,
    /// Optional scripted disable-read responses; pop from the front. Falls back
    /// to `disabled.contains_key` once exhausted.
    scripted_disable_reads: Rc<RefCell<VecDeque<bool>>>,
}

impl FakeAgentStore {
    pub fn new() -> Self {
        Self {
            scopes: Rc::new(RefCell::new(HashMap::new())),
            scope_writes: Rc::new(RefCell::new(Vec::new())),
            pending: Rc::new(RefCell::new(HashMap::new())),
            disabled: Rc::new(RefCell::new(HashMap::new())),
            disable_writes: Rc::new(RefCell::new(Vec::new())),
            fail_scope_read: Rc::new(RefCell::new(false)),
            fail_scope_write: Rc::new(RefCell::new(false)),
            fail_pending_clear: Rc::new(RefCell::new(false)),
            fail_pending_write: Rc::new(RefCell::new(false)),
            fail_disable_read: Rc::new(RefCell::new(false)),
            fail_disable_write: Rc::new(RefCell::new(false)),
            scripted_disable_reads: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    pub fn with_scope(self, id: AgentId, scope: Scope) -> Self {
        self.scopes.borrow_mut().insert(id, scope);
        self
    }

    pub fn with_disabled(self, id: AgentId) -> Self {
        self.disabled.borrow_mut().insert(id, ());
        self
    }

    /// Scripted disable-read responses — pop one boolean per call. Used to
    /// model state that changes between sequential reads (e.g. disable marker
    /// appearing during a lock wait).
    pub fn with_scripted_disable_reads(self, values: Vec<bool>) -> Self {
        *self.scripted_disable_reads.borrow_mut() = values.into_iter().collect();
        self
    }

    pub fn fail_scope_reads(self) -> Self {
        *self.fail_scope_read.borrow_mut() = true;
        self
    }

    pub fn fail_scope_writes(self) -> Self {
        *self.fail_scope_write.borrow_mut() = true;
        self
    }

    pub fn with_pending(self, parent: AgentId, scope: Scope) -> Self {
        self.pending.borrow_mut().insert(parent, scope);
        self
    }

    pub fn fail_pending_clears(self) -> Self {
        *self.fail_pending_clear.borrow_mut() = true;
        self
    }

    pub fn fail_pending_writes(self) -> Self {
        *self.fail_pending_write.borrow_mut() = true;
        self
    }

    /// Shared handle to the pending map.
    pub fn pending(&self) -> Rc<RefCell<HashMap<AgentId, Scope>>> {
        Rc::clone(&self.pending)
    }

    pub fn fail_disable_reads(self) -> Self {
        *self.fail_disable_read.borrow_mut() = true;
        self
    }

    pub fn fail_disable_writes(self) -> Self {
        *self.fail_disable_write.borrow_mut() = true;
        self
    }

    pub fn scope_writes(&self) -> Rc<RefCell<Vec<(AgentId, Scope)>>> {
        Rc::clone(&self.scope_writes)
    }

    pub fn disable_writes(&self) -> Rc<RefCell<Vec<AgentId>>> {
        Rc::clone(&self.disable_writes)
    }

    /// Shared handle to the scope map — used to mutate state from outside
    /// `process_action` (e.g. in a `StubLock` side-effect callback).
    pub fn scopes(&self) -> Rc<RefCell<HashMap<AgentId, Scope>>> {
        Rc::clone(&self.scopes)
    }

    pub fn disabled(&self) -> Rc<RefCell<HashMap<AgentId, ()>>> {
        Rc::clone(&self.disabled)
    }
}

impl AgentStore for FakeAgentStore {
    fn scope_for(&self, agent_id: &AgentId) -> Result<Option<Scope>, ScopeStoreError> {
        if *self.fail_scope_read.borrow() {
            return Err(ScopeStoreError::Unavailable(
                "fake scope read failure".into(),
            ));
        }
        Ok(self.scopes.borrow().get(agent_id).cloned())
    }

    fn assign_scope(&self, agent_id: &AgentId, scope: Scope) -> Result<(), ScopeStoreError> {
        if *self.fail_scope_write.borrow() {
            return Err(ScopeStoreError::Unavailable(
                "fake scope write failure".into(),
            ));
        }
        self.scope_writes
            .borrow_mut()
            .push((agent_id.clone(), scope.clone()));
        self.scopes.borrow_mut().insert(agent_id.clone(), scope);
        Ok(())
    }

    fn set_pending(&self, parent: &AgentId, scope: Scope) -> Result<(), ScopeStoreError> {
        if *self.fail_pending_write.borrow() {
            return Err(ScopeStoreError::Unavailable(
                "fake pending write failure".into(),
            ));
        }
        self.pending.borrow_mut().insert(parent.clone(), scope);
        Ok(())
    }

    fn take_pending(&self, parent: &AgentId) -> Result<Option<Scope>, ScopeStoreError> {
        Ok(self.pending.borrow_mut().remove(parent))
    }

    fn clear_pending(&self, parent: &AgentId) -> Result<(), ScopeStoreError> {
        if *self.fail_pending_clear.borrow() {
            return Err(ScopeStoreError::Unavailable(
                "fake pending clear failure".into(),
            ));
        }
        self.pending.borrow_mut().remove(parent);
        Ok(())
    }

    fn is_disabled(&self, agent_id: &AgentId) -> Result<bool, ScopeStoreError> {
        if *self.fail_disable_read.borrow() {
            return Err(ScopeStoreError::Unavailable(
                "fake disable read failure".into(),
            ));
        }
        if let Some(scripted) = self.scripted_disable_reads.borrow_mut().pop_front() {
            return Ok(scripted);
        }
        Ok(self.disabled.borrow().contains_key(agent_id))
    }

    fn disable(&self, agent_id: &AgentId) -> Result<(), ScopeStoreError> {
        if *self.fail_disable_write.borrow() {
            return Err(ScopeStoreError::Unavailable(
                "fake disable write failure".into(),
            ));
        }
        self.disable_writes.borrow_mut().push(agent_id.clone());
        self.disabled.borrow_mut().insert(agent_id.clone(), ());
        Ok(())
    }
}

/// Snapshot of a logged Verdict — the recording fake captures only the kind so
/// tests can assert outcomes without cloning Verdict (which is non-Clone).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoggedVerdict {
    Green,
    Red(RedKind),
}

impl From<&Verdict> for LoggedVerdict {
    fn from(v: &Verdict) -> Self {
        match v {
            Verdict::Green => Self::Green,
            Verdict::Red { kind, .. } => Self::Red(kind.clone()),
        }
    }
}

/// In-memory `ClassificationHistoryWriter` for tests. Captures every
/// `write` record and `write_event` event. Methods return `Ok(())`.
pub struct RecordingHistory {
    records: Rc<RefCell<Vec<domain::ClassificationRecord>>>,
    events: Rc<RefCell<Vec<domain::ClassificationEvent>>>,
}

impl RecordingHistory {
    pub fn new() -> Self {
        Self {
            records: Rc::new(RefCell::new(Vec::new())),
            events: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn records(&self) -> Rc<RefCell<Vec<domain::ClassificationRecord>>> {
        Rc::clone(&self.records)
    }

    pub fn events(&self) -> Rc<RefCell<Vec<domain::ClassificationEvent>>> {
        Rc::clone(&self.events)
    }

    pub fn event_messages(&self) -> Vec<String> {
        self.events
            .borrow()
            .iter()
            .map(|e| e.message.clone())
            .collect()
    }

    pub fn state_transition_messages(&self) -> Vec<String> {
        self.events
            .borrow()
            .iter()
            .filter(|e| matches!(e.kind, domain::EventKind::StateTransition))
            .map(|e| e.message.clone())
            .collect()
    }
}

impl ClassificationHistoryWriter for RecordingHistory {
    fn write(&self, record: domain::ClassificationRecord) -> Result<(), domain::HistoryError> {
        self.records.borrow_mut().push(record);
        Ok(())
    }

    fn write_event(&self, event: domain::ClassificationEvent) -> Result<(), domain::HistoryError> {
        self.events.borrow_mut().push(event);
        Ok(())
    }
}

/// Stub `Prompter` driven by pre-loaded queues — one queue per method. Tests
/// push the outcomes they expect in order; calls panic if the queue is empty
/// (signals a missing test setup).
pub struct StubPrompter {
    choose: RefCell<VecDeque<Result<VerdictChoice, PromptError>>>,
    update: RefCell<VecDeque<Result<Scope, PromptError>>>,
    confirm: RefCell<VecDeque<Result<bool, PromptError>>>,
    choose_calls: Rc<RefCell<usize>>,
    update_calls: Rc<RefCell<usize>>,
    confirm_calls: Rc<RefCell<usize>>,
    /// Last `Action` passed to `choose`, captured for assertions. Snapshots the
    /// kind + args by string value since `Action` is non-Clone.
    last_choose_action: Rc<RefCell<Option<(String, String)>>>,
}

impl StubPrompter {
    pub fn new() -> Self {
        Self {
            choose: RefCell::new(VecDeque::new()),
            update: RefCell::new(VecDeque::new()),
            confirm: RefCell::new(VecDeque::new()),
            choose_calls: Rc::new(RefCell::new(0)),
            update_calls: Rc::new(RefCell::new(0)),
            confirm_calls: Rc::new(RefCell::new(0)),
            last_choose_action: Rc::new(RefCell::new(None)),
        }
    }

    pub fn last_choose_action(&self) -> Rc<RefCell<Option<(String, String)>>> {
        Rc::clone(&self.last_choose_action)
    }

    pub fn with_choose(self, choice: VerdictChoice) -> Self {
        self.choose.borrow_mut().push_back(Ok(choice));
        self
    }

    pub fn with_choose_cancelled(self) -> Self {
        self.choose
            .borrow_mut()
            .push_back(Err(PromptError::Cancelled));
        self
    }

    pub fn with_choose_unavailable(self) -> Self {
        self.choose
            .borrow_mut()
            .push_back(Err(PromptError::Unavailable("fake choose failure".into())));
        self
    }

    pub fn with_update_scope(self, scope: Scope) -> Self {
        self.update.borrow_mut().push_back(Ok(scope));
        self
    }

    pub fn with_update_cancelled(self) -> Self {
        self.update
            .borrow_mut()
            .push_back(Err(PromptError::Cancelled));
        self
    }

    pub fn with_update_unavailable(self) -> Self {
        self.update
            .borrow_mut()
            .push_back(Err(PromptError::Unavailable("fake update failure".into())));
        self
    }

    pub fn with_confirm(self, value: bool) -> Self {
        self.confirm.borrow_mut().push_back(Ok(value));
        self
    }

    pub fn with_confirm_unavailable(self) -> Self {
        self.confirm
            .borrow_mut()
            .push_back(Err(PromptError::Unavailable("fake confirm failure".into())));
        self
    }

    pub fn choose_calls(&self) -> Rc<RefCell<usize>> {
        Rc::clone(&self.choose_calls)
    }

    pub fn update_calls(&self) -> Rc<RefCell<usize>> {
        Rc::clone(&self.update_calls)
    }

    pub fn confirm_calls(&self) -> Rc<RefCell<usize>> {
        Rc::clone(&self.confirm_calls)
    }
}

impl Prompter for StubPrompter {
    fn choose(
        &self,
        _scope: &Scope,
        action: &Action,
        _reason: &str,
        _cwd: &Path,
    ) -> Result<VerdictChoice, PromptError> {
        *self.choose_calls.borrow_mut() += 1;
        *self.last_choose_action.borrow_mut() = Some((
            action.kind.as_ref().to_string(),
            action.args.as_ref().to_string(),
        ));
        self.choose
            .borrow_mut()
            .pop_front()
            .expect("StubPrompter: choose called with empty queue")
    }

    fn update_scope(
        &self,
        _current: Option<&Scope>,
        _working_dir: &Path,
    ) -> Result<Scope, PromptError> {
        *self.update_calls.borrow_mut() += 1;
        self.update
            .borrow_mut()
            .pop_front()
            .expect("StubPrompter: update_scope called with empty queue")
    }

    fn confirm_disable(&self) -> Result<bool, PromptError> {
        *self.confirm_calls.borrow_mut() += 1;
        self.confirm
            .borrow_mut()
            .pop_front()
            .expect("StubPrompter: confirm_disable called with empty queue")
    }
}

/// Stub `PromptLock` returning a unit guard. Default = always-acquire-Ok;
/// `failing()` builds a stub that returns `PromptError::Unavailable`;
/// `with_side_effect(F)` runs a closure on each successful acquire — used to
/// simulate state changes that happen "while we waited" for the lock (e.g.
/// another owner setting the disable marker or rewriting the scope).
pub struct StubLock {
    fail: bool,
    on_acquire: Option<Box<dyn Fn()>>,
    calls: Rc<RefCell<usize>>,
}

impl StubLock {
    pub fn new() -> Self {
        Self {
            fail: false,
            on_acquire: None,
            calls: Rc::new(RefCell::new(0)),
        }
    }

    pub fn failing() -> Self {
        Self {
            fail: true,
            on_acquire: None,
            calls: Rc::new(RefCell::new(0)),
        }
    }

    pub fn with_side_effect<F: Fn() + 'static>(f: F) -> Self {
        Self {
            fail: false,
            on_acquire: Some(Box::new(f)),
            calls: Rc::new(RefCell::new(0)),
        }
    }

    pub fn call_count(&self) -> Rc<RefCell<usize>> {
        Rc::clone(&self.calls)
    }
}

impl PromptLock for StubLock {
    type Guard = ();
    fn acquire(&self, _agent_id: &AgentId) -> Result<(), PromptError> {
        *self.calls.borrow_mut() += 1;
        if self.fail {
            return Err(PromptError::Unavailable("fake lock failure".into()));
        }
        if let Some(f) = &self.on_acquire {
            f();
        }
        Ok(())
    }
}

pub enum LlmOutcome {
    Green,
    Red(String),
    Unavailable,
}

/// Scripted LLM classifier. Each constructor seeds the first outcome; chained
/// `.then_*` calls append subsequent outcomes. When the queue is exhausted,
/// the last popped outcome is repeated — so single-call tests can keep using
/// the simple `green()` / `red(_)` form without configuring a default.
pub struct FakeLlmClassifier {
    sequence: RefCell<VecDeque<LlmOutcome>>,
    last: RefCell<Option<LlmOutcome>>,
    calls: Rc<RefCell<usize>>,
    last_standing: Rc<RefCell<StandingGuidance>>,
    extraction: ExtractionOutcome,
    extract_texts: Rc<RefCell<Vec<String>>>,
    extract_side_effect: Option<Box<dyn Fn()>>,
}

/// Scripted answer for [`LlmClassifier::extract_scope`].
pub enum ExtractionOutcome {
    Scope(Scope),
    NoScope,
    Unavailable,
}

impl FakeLlmClassifier {
    pub fn green() -> Self {
        Self::seeded(LlmOutcome::Green)
    }

    pub fn red(reason: &str) -> Self {
        Self::seeded(LlmOutcome::Red(reason.into()))
    }

    pub fn unavailable() -> Self {
        Self::seeded(LlmOutcome::Unavailable)
    }

    fn seeded(first: LlmOutcome) -> Self {
        let mut q = VecDeque::new();
        q.push_back(first);
        Self {
            sequence: RefCell::new(q),
            last: RefCell::new(None),
            calls: Rc::new(RefCell::new(0)),
            last_standing: Rc::new(RefCell::new(StandingGuidance::default())),
            extraction: ExtractionOutcome::Unavailable,
            extract_texts: Rc::new(RefCell::new(Vec::new())),
            extract_side_effect: None,
        }
    }

    /// Fake whose `extract_scope` answers with `outcome`. `classify` answers
    /// Green.
    pub fn extracting(outcome: ExtractionOutcome) -> Self {
        let mut me = Self::green();
        me.extraction = outcome;
        me
    }

    /// Runs `f` inside every `extract_scope` call, before it returns. Models
    /// state that changes while the LLM call is in flight.
    pub fn with_extract_side_effect<F: Fn() + 'static>(mut self, f: F) -> Self {
        self.extract_side_effect = Some(Box::new(f));
        self
    }

    /// Every text passed to `extract_scope`, in call order.
    pub fn extract_texts(&self) -> Rc<RefCell<Vec<String>>> {
        Rc::clone(&self.extract_texts)
    }

    pub fn then_green(self) -> Self {
        self.sequence.borrow_mut().push_back(LlmOutcome::Green);
        self
    }

    pub fn then_red(self, reason: &str) -> Self {
        self.sequence
            .borrow_mut()
            .push_back(LlmOutcome::Red(reason.into()));
        self
    }

    pub fn call_count(&self) -> Rc<RefCell<usize>> {
        Rc::clone(&self.calls)
    }

    /// Captures the most recent `ctx.standing` passed to `classify`. Tests use
    /// this to assert process_action threaded the reader's output through
    /// `ClassificationContext`.
    pub fn last_standing(&self) -> Rc<RefCell<StandingGuidance>> {
        Rc::clone(&self.last_standing)
    }
}

impl LlmClassifier for FakeLlmClassifier {
    fn classify(
        &self,
        ctx: &ClassificationContext,
    ) -> Result<ClassifierOutcome, LlmClassifierError> {
        *self.calls.borrow_mut() += 1;
        *self.last_standing.borrow_mut() = ctx.standing.clone();
        if let Some(next) = self.sequence.borrow_mut().pop_front() {
            *self.last.borrow_mut() = Some(next);
        }
        let stats = LlmCallStats {
            model: "fake-classifier".into(),
            latency: std::time::Duration::from_millis(1),
            input_tokens: Some(10),
            output_tokens: Some(5),
        };
        match self
            .last
            .borrow()
            .as_ref()
            .expect("FakeLlmClassifier: classify called with no outcome configured")
        {
            LlmOutcome::Green => Ok(ClassifierOutcome {
                verdict: Verdict::Green,
                raw_verdict: RawVerdict::Green,
                raw_reason: None,
                llm: Some(stats),
            }),
            LlmOutcome::Red(reason) => Ok(ClassifierOutcome {
                verdict: Verdict::Red {
                    kind: RedKind::OutOfScope,
                    reason: reason.clone(),
                },
                raw_verdict: RawVerdict::Red,
                raw_reason: Some(reason.clone()),
                llm: Some(stats),
            }),
            LlmOutcome::Unavailable => {
                Err(LlmClassifierError::Unavailable("fake LLM failure".into()))
            }
        }
    }

    fn extract_scope(&self, text: &str) -> Result<Option<Scope>, LlmClassifierError> {
        self.extract_texts.borrow_mut().push(text.to_string());
        if let Some(side_effect) = &self.extract_side_effect {
            side_effect();
        }
        match &self.extraction {
            ExtractionOutcome::Scope(scope) => Ok(Some(scope.clone())),
            ExtractionOutcome::NoScope => Ok(None),
            ExtractionOutcome::Unavailable => {
                Err(LlmClassifierError::Unavailable("fake LLM failure".into()))
            }
        }
    }
}

pub enum GuidanceOutcome {
    None,
    SomeProject(String),
    Unavailable,
}

pub struct FakeGuidanceReader {
    outcome: GuidanceOutcome,
}

impl FakeGuidanceReader {
    pub fn none() -> Self {
        Self {
            outcome: GuidanceOutcome::None,
        }
    }

    /// Populate the `project` slot (REMIT.md content).
    pub fn some(text: &str) -> Self {
        Self {
            outcome: GuidanceOutcome::SomeProject(text.into()),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            outcome: GuidanceOutcome::Unavailable,
        }
    }
}

impl StandingGuidanceReader for FakeGuidanceReader {
    fn read(&self) -> Result<StandingGuidance, StandingGuidanceError> {
        match &self.outcome {
            GuidanceOutcome::None => Ok(StandingGuidance::default()),
            GuidanceOutcome::SomeProject(s) => Ok(StandingGuidance {
                project: Some(s.clone()),
                agent_template: None,
            }),
            GuidanceOutcome::Unavailable => Err(StandingGuidanceError::Unavailable(
                "fake guidance failure".into(),
            )),
        }
    }
}
