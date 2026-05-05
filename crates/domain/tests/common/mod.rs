// Each integration-test binary in `tests/` only uses a subset of these helpers;
// allow per-binary dead code rather than splitting fakes across files.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use domain::{
    AgentId, ClassificationInput, ClassifierLogger, PromptError, Scope, ScopePrompter, ScopeReader,
    ScopeStoreError, ScopeWriter,
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

pub struct RecordingLogger {
    calls: Rc<RefCell<usize>>,
}

impl RecordingLogger {
    pub fn new() -> Self {
        Self {
            calls: Rc::new(RefCell::new(0)),
        }
    }

    pub fn call_count(&self) -> Rc<RefCell<usize>> {
        Rc::clone(&self.calls)
    }
}

impl ClassifierLogger for RecordingLogger {
    fn log_incoming(&self, _input: &ClassificationInput) {
        *self.calls.borrow_mut() += 1;
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
