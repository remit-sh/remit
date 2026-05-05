// Each integration-test binary in `tests/` only uses a subset of these helpers;
// allow per-binary dead code rather than splitting fakes across files.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use domain::{
    AgentId, ClassificationInput, ClassifierLogger, PromptError, RedKind, Scope, ScopePrompter,
    ScopeReader, ScopeStoreError, ScopeWriter, Verdict,
};

pub struct FakeReader {
    map: HashMap<AgentId, Scope>,
    fail_for: HashSet<AgentId>,
    fail_all: bool,
}

impl FakeReader {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            fail_for: HashSet::new(),
            fail_all: false,
        }
    }

    pub fn with(mut self, id: AgentId, scope: Scope) -> Self {
        self.map.insert(id, scope);
        self
    }

    pub fn failing() -> Self {
        Self {
            map: HashMap::new(),
            fail_for: HashSet::new(),
            fail_all: true,
        }
    }

    pub fn failing_for(mut self, id: AgentId) -> Self {
        self.fail_for.insert(id);
        self
    }
}

impl ScopeReader for FakeReader {
    fn scope_for(&self, agent_id: &AgentId) -> Result<Option<Scope>, ScopeStoreError> {
        if self.fail_all || self.fail_for.contains(agent_id) {
            return Err(ScopeStoreError::Unavailable("fake failure".into()));
        }
        Ok(self.map.get(agent_id).cloned())
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

pub struct RecordingLogger {
    incoming: Rc<RefCell<usize>>,
    outcomes: Rc<RefCell<Vec<LoggedVerdict>>>,
    errors: Rc<RefCell<Vec<String>>>,
}

impl RecordingLogger {
    pub fn new() -> Self {
        Self {
            incoming: Rc::new(RefCell::new(0)),
            outcomes: Rc::new(RefCell::new(Vec::new())),
            errors: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn call_count(&self) -> Rc<RefCell<usize>> {
        Rc::clone(&self.incoming)
    }

    pub fn outcomes(&self) -> Rc<RefCell<Vec<LoggedVerdict>>> {
        Rc::clone(&self.outcomes)
    }

    pub fn errors(&self) -> Rc<RefCell<Vec<String>>> {
        Rc::clone(&self.errors)
    }
}

impl ClassifierLogger for RecordingLogger {
    fn log_incoming(&self, _input: &ClassificationInput) {
        *self.incoming.borrow_mut() += 1;
    }

    fn log_outcome(&self, _input: &ClassificationInput, verdict: &Verdict) {
        self.outcomes.borrow_mut().push(verdict.into());
    }

    fn log_error(&self, _input: &ClassificationInput, error: &(dyn std::error::Error + 'static)) {
        let mut chain = error.to_string();
        let mut current = error.source();
        while let Some(c) = current {
            chain.push_str(": ");
            chain.push_str(&c.to_string());
            current = c.source();
        }
        self.errors.borrow_mut().push(chain);
    }
}

pub enum PromptOutcome {
    Returns(Scope),
    Cancelled,
    Unavailable,
}

pub struct FakePrompter {
    outcome: PromptOutcome,
    calls: Rc<RefCell<usize>>,
}

impl FakePrompter {
    pub fn returning(scope: Scope) -> Self {
        Self::new(PromptOutcome::Returns(scope))
    }

    pub fn cancelled() -> Self {
        Self::new(PromptOutcome::Cancelled)
    }

    pub fn unavailable() -> Self {
        Self::new(PromptOutcome::Unavailable)
    }

    fn new(outcome: PromptOutcome) -> Self {
        Self {
            outcome,
            calls: Rc::new(RefCell::new(0)),
        }
    }

    pub fn call_count(&self) -> Rc<RefCell<usize>> {
        Rc::clone(&self.calls)
    }
}

impl ScopePrompter for FakePrompter {
    fn prompt(&self, _agent_id: &AgentId) -> Result<Scope, PromptError> {
        *self.calls.borrow_mut() += 1;
        match &self.outcome {
            PromptOutcome::Returns(s) => Ok(s.clone()),
            PromptOutcome::Cancelled => Err(PromptError::Cancelled),
            PromptOutcome::Unavailable => Err(PromptError::Unavailable("fake failure".into())),
        }
    }
}

pub struct RecordingWriter {
    writes: Rc<RefCell<Vec<(AgentId, Scope)>>>,
    fail: bool,
}

impl RecordingWriter {
    pub fn new() -> Self {
        Self {
            writes: Rc::new(RefCell::new(Vec::new())),
            fail: false,
        }
    }

    pub fn failing() -> Self {
        Self {
            writes: Rc::new(RefCell::new(Vec::new())),
            fail: true,
        }
    }

    pub fn writes(&self) -> Rc<RefCell<Vec<(AgentId, Scope)>>> {
        Rc::clone(&self.writes)
    }
}

impl ScopeWriter for RecordingWriter {
    fn assign(&self, agent_id: &AgentId, scope: Scope) -> Result<(), ScopeStoreError> {
        if self.fail {
            return Err(ScopeStoreError::Unavailable("fake writer failure".into()));
        }
        self.writes.borrow_mut().push((agent_id.clone(), scope));
        Ok(())
    }
}
