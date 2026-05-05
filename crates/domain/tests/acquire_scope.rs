use domain::{AcquireScopeError, Agent, AgentId, Scope, acquire_scope};

mod common;
use common::{FakePrompter, FakeReader, RecordingWriter};

fn id(s: &str) -> AgentId {
    AgentId::try_new(s).unwrap()
}

fn scope(s: &str) -> Scope {
    Scope::try_new(s).unwrap()
}

#[test]
fn returns_existing_scope_without_prompting_or_writing() {
    let existing = scope("existing");
    let reader = FakeReader::new().with(id("sess"), existing.clone());
    let prompter = FakePrompter::returning(scope("should not be used"));
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let got = acquire_scope(&reader, &prompter, &writer, &Agent::root(id("sess"))).unwrap();

    assert_eq!(got, Some(existing));
    assert_eq!(*prompt_calls.borrow(), 0);
    assert!(writes.borrow().is_empty());
}

#[test]
fn subagent_without_own_scope_returns_none_without_prompting_or_falling_back_to_parent() {
    // Parent has a scope but child does not — under bundled-scope, child gets no
    // inheritance; the start-event hook should have already materialised the child's row.
    let reader = FakeReader::new().with(id("parent"), scope("parent task"));
    let prompter = FakePrompter::returning(scope("nope"));
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let got = acquire_scope(
        &reader,
        &prompter,
        &writer,
        &Agent::subagent(id("child"), id("parent")),
    )
    .unwrap();

    assert_eq!(got, None);
    assert_eq!(*prompt_calls.borrow(), 0);
    assert!(writes.borrow().is_empty());
}

#[test]
fn root_with_missing_scope_prompts_writes_and_returns_new_scope() {
    let reader = FakeReader::new();
    let new_scope = scope("freshly typed");
    let prompter = FakePrompter::returning(new_scope.clone());
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let got = acquire_scope(&reader, &prompter, &writer, &Agent::root(id("root"))).unwrap();

    assert_eq!(got, Some(new_scope.clone()));
    assert_eq!(*prompt_calls.borrow(), 1);
    let recorded = writes.borrow();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].0, id("root"));
    assert_eq!(recorded[0].1, new_scope);
}

#[test]
fn cancelled_prompt_returns_none_without_writing() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::cancelled();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let got = acquire_scope(&reader, &prompter, &writer, &Agent::root(id("root"))).unwrap();

    assert_eq!(got, None);
    assert!(writes.borrow().is_empty());
}

#[test]
fn unavailable_prompt_returns_prompt_error_without_writing() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::unavailable();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let result = acquire_scope(&reader, &prompter, &writer, &Agent::root(id("root")));

    assert!(matches!(result, Err(AcquireScopeError::Prompt(_))));
    assert!(writes.borrow().is_empty());
}

#[test]
fn store_read_error_returns_store_error() {
    let reader = FakeReader::failing();
    let prompter = FakePrompter::returning(scope("nope"));
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();

    let result = acquire_scope(&reader, &prompter, &writer, &Agent::root(id("root")));

    assert!(matches!(result, Err(AcquireScopeError::Store(_))));
    assert_eq!(*prompt_calls.borrow(), 0);
}

#[test]
fn store_write_error_returns_store_error() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::returning(scope("anything"));
    let writer = RecordingWriter::failing();

    let result = acquire_scope(&reader, &prompter, &writer, &Agent::root(id("root")));

    assert!(matches!(result, Err(AcquireScopeError::Store(_))));
}
