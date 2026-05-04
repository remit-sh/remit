// Each integration-test binary in `tests/` only uses a subset of these helpers;
// allow per-binary dead code rather than splitting fakes across files.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use domain::{ClassificationInput, ClassifierLogger, Scope, ScopeError, ScopeReader, SessionId};

pub struct FakeReader {
    map: HashMap<SessionId, Scope>,
    fail_for: HashSet<SessionId>,
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

    pub fn with(mut self, id: SessionId, scope: Scope) -> Self {
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

    pub fn failing_for(mut self, id: SessionId) -> Self {
        self.fail_for.insert(id);
        self
    }
}

impl ScopeReader for FakeReader {
    fn scope_for(&self, session_id: &SessionId) -> Result<Option<Scope>, ScopeError> {
        if self.fail_all || self.fail_for.contains(session_id) {
            return Err(ScopeError::Unavailable("fake failure".into()));
        }
        Ok(self.map.get(session_id).cloned())
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
