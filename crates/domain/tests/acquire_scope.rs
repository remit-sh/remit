use domain::{AcquireScopeError, Scope, Session, SessionId, acquire_scope};

mod common;
use common::{FakePrompter, FakeReader, RecordingWriter};

fn session(id: &str, parent: Option<&str>) -> Session {
    Session {
        id: SessionId::try_new(id).unwrap(),
        parent_id: parent.map(|p| SessionId::try_new(p).unwrap()),
    }
}

#[test]
fn returns_existing_scope_without_prompting_or_writing() {
    let id = SessionId::try_new("sess").unwrap();
    let scope = Scope::new("existing");
    let reader = FakeReader::new().with(id, scope.clone());
    let prompter = FakePrompter::returning(Scope::new("should not be used"));
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let got = acquire_scope(&reader, &prompter, &writer, &session("sess", None)).unwrap();

    assert_eq!(got, Some(scope));
    assert_eq!(*prompt_calls.borrow(), 0);
    assert!(writes.borrow().is_empty());
}

#[test]
fn subagent_with_parent_scope_does_not_inherit() {
    // Even if the parent has a scope, the subagent does not silently inherit:
    // each spawn must own its scope (set via `prepare_next_scope`).
    let parent_id = SessionId::try_new("parent").unwrap();
    let reader = FakeReader::new().with(parent_id, Scope::new("parent task"));
    let prompter = FakePrompter::returning(Scope::new("nope"));
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let got = acquire_scope(
        &reader,
        &prompter,
        &writer,
        &session("child", Some("parent")),
    )
    .unwrap();

    assert_eq!(got, None);
    assert_eq!(*prompt_calls.borrow(), 0);
    assert!(writes.borrow().is_empty());
}

#[test]
fn subagent_with_no_parent_scope_returns_none_without_prompting() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::returning(Scope::new("nope"));
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let got = acquire_scope(
        &reader,
        &prompter,
        &writer,
        &session("child", Some("parent")),
    )
    .unwrap();

    assert_eq!(got, None);
    assert_eq!(*prompt_calls.borrow(), 0);
    assert!(writes.borrow().is_empty());
}

#[test]
fn root_with_missing_scope_prompts_writes_and_returns_new_scope() {
    let reader = FakeReader::new();
    let scope = Scope::new("freshly typed");
    let prompter = FakePrompter::returning(scope.clone());
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let got = acquire_scope(&reader, &prompter, &writer, &session("root", None)).unwrap();

    assert_eq!(got, Some(scope.clone()));
    assert_eq!(*prompt_calls.borrow(), 1);
    let recorded = writes.borrow();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].0, SessionId::try_new("root").unwrap());
    assert_eq!(recorded[0].1, scope);
}

#[test]
fn cancelled_prompt_returns_none_without_writing() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::cancelled();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let got = acquire_scope(&reader, &prompter, &writer, &session("root", None)).unwrap();

    assert_eq!(got, None);
    assert!(writes.borrow().is_empty());
}

#[test]
fn unavailable_prompt_returns_prompt_error_without_writing() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::unavailable();
    let writer = RecordingWriter::new();
    let writes = writer.writes();

    let result = acquire_scope(&reader, &prompter, &writer, &session("root", None));

    assert!(matches!(result, Err(AcquireScopeError::Prompt(_))));
    assert!(writes.borrow().is_empty());
}

#[test]
fn store_read_error_returns_store_error() {
    let reader = FakeReader::failing();
    let prompter = FakePrompter::returning(Scope::new("nope"));
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();

    let result = acquire_scope(&reader, &prompter, &writer, &session("root", None));

    assert!(matches!(result, Err(AcquireScopeError::Store(_))));
    assert_eq!(*prompt_calls.borrow(), 0);
}

#[test]
fn store_write_error_returns_store_error() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::returning(Scope::new("anything"));
    let writer = RecordingWriter::failing();

    let result = acquire_scope(&reader, &prompter, &writer, &session("root", None));

    assert!(matches!(result, Err(AcquireScopeError::Store(_))));
}
