use domain::{
    Agent, AgentId, AssignExtractedScopeResult, Scope, StageExtractedScopeResult,
    assign_extracted_scope, stage_extracted_scope,
};

mod common;
use common::{ExtractionOutcome, FakeAgentStore, FakeLlmClassifier};

fn id(s: &str) -> AgentId {
    AgentId::try_new(s).unwrap()
}

fn scope(s: &str) -> Scope {
    Scope::try_new(s).unwrap()
}

fn extracting(s: &str) -> FakeLlmClassifier {
    FakeLlmClassifier::extracting(ExtractionOutcome::Scope(scope(s)))
}

#[test]
fn root_without_scope_is_assigned_the_extracted_scope() {
    let store = FakeAgentStore::new();
    let llm = extracting("Fix the failing test in src/todos.ts");

    let outcome = assign_extracted_scope(
        &store,
        &llm,
        &Agent::root(id("root")),
        "fix the failing test in src/todos.ts",
    );

    assert!(matches!(outcome, AssignExtractedScopeResult::Assigned));
    assert_eq!(
        store.scopes().borrow().get(&id("root")),
        Some(&scope("Fix the failing test in src/todos.ts"))
    );
    assert_eq!(
        *llm.extract_texts().borrow(),
        vec!["fix the failing test in src/todos.ts".to_string()]
    );
}

#[test]
fn root_with_a_scope_keeps_it_and_the_llm_is_not_called() {
    let store = FakeAgentStore::new().with_scope(id("root"), scope("original task"));
    let llm = extracting("a different task");

    let outcome = assign_extracted_scope(&store, &llm, &Agent::root(id("root")), "do another");

    assert!(matches!(outcome, AssignExtractedScopeResult::AlreadyScoped));
    assert_eq!(
        store.scopes().borrow().get(&id("root")),
        Some(&scope("original task"))
    );
    assert!(llm.extract_texts().borrow().is_empty());
}

#[test]
fn subagent_is_never_assigned_an_extracted_scope() {
    let store = FakeAgentStore::new();
    let llm = extracting("some task");

    let outcome = assign_extracted_scope(
        &store,
        &llm,
        &Agent::subagent(id("child"), id("parent")),
        "some task",
    );

    assert!(matches!(outcome, AssignExtractedScopeResult::NotRoot));
    assert!(store.scope_writes().borrow().is_empty());
    assert!(llm.extract_texts().borrow().is_empty());
}

#[test]
fn text_holding_no_scope_assigns_nothing() {
    let store = FakeAgentStore::new();
    let llm = FakeLlmClassifier::extracting(ExtractionOutcome::NoScope);

    let outcome = assign_extracted_scope(&store, &llm, &Agent::root(id("root")), "hi");

    assert!(matches!(outcome, AssignExtractedScopeResult::NoScope));
    assert!(store.scope_writes().borrow().is_empty());
}

#[test]
fn extraction_failure_assigns_nothing() {
    let store = FakeAgentStore::new();
    let llm = FakeLlmClassifier::extracting(ExtractionOutcome::Unavailable);

    let outcome = assign_extracted_scope(&store, &llm, &Agent::root(id("root")), "fix it");

    assert!(matches!(
        outcome,
        AssignExtractedScopeResult::ExtractionUnavailable(_)
    ));
    assert!(store.scope_writes().borrow().is_empty());
}

#[test]
fn scope_read_failure_assigns_nothing_and_the_llm_is_not_called() {
    let store = FakeAgentStore::new().fail_scope_reads();
    let llm = extracting("some task");

    let outcome = assign_extracted_scope(&store, &llm, &Agent::root(id("root")), "some task");

    assert!(matches!(
        outcome,
        AssignExtractedScopeResult::StoreUnavailable(_)
    ));
    assert!(store.scope_writes().borrow().is_empty());
    assert!(llm.extract_texts().borrow().is_empty());
}

#[test]
fn scope_write_failure_is_reported() {
    let store = FakeAgentStore::new().fail_scope_writes();
    let llm = extracting("some task");

    let outcome = assign_extracted_scope(&store, &llm, &Agent::root(id("root")), "some task");

    assert!(matches!(
        outcome,
        AssignExtractedScopeResult::StoreUnavailable(_)
    ));
}

#[test]
fn scope_set_while_the_llm_call_is_in_flight_is_not_replaced() {
    let store = FakeAgentStore::new();
    let scopes = store.scopes();
    let llm = extracting("extracted task").with_extract_side_effect(move || {
        scopes
            .borrow_mut()
            .insert(id("root"), scope("typed into the dialog"));
    });

    let outcome = assign_extracted_scope(&store, &llm, &Agent::root(id("root")), "some task");

    assert!(matches!(outcome, AssignExtractedScopeResult::AlreadyScoped));
    assert_eq!(
        store.scopes().borrow().get(&id("root")),
        Some(&scope("typed into the dialog"))
    );
}

#[test]
fn bundled_text_is_staged_as_its_extracted_scope() {
    let store = FakeAgentStore::new();
    let llm = extracting("update ticket status");

    let outcome = stage_extracted_scope(
        &store,
        &llm,
        &id("parent"),
        "update ticket status — this is for cleanup",
    );

    assert!(matches!(outcome, StageExtractedScopeResult::Staged));
    assert_eq!(
        store.pending().borrow().get(&id("parent")),
        Some(&scope("update ticket status"))
    );
    assert_eq!(
        *llm.extract_texts().borrow(),
        vec!["update ticket status — this is for cleanup".to_string()]
    );
}

#[test]
fn bundled_text_holding_no_scope_stages_nothing_and_clears_a_stale_slot() {
    let store = FakeAgentStore::new().with_pending(id("parent"), scope("earlier round"));
    let llm = FakeLlmClassifier::extracting(ExtractionOutcome::NoScope);

    let outcome = stage_extracted_scope(&store, &llm, &id("parent"), "thanks!");

    assert!(matches!(outcome, StageExtractedScopeResult::NoScope));
    assert!(store.pending().borrow().is_empty());
}

#[test]
fn bundled_extraction_failure_stages_nothing_and_clears_a_stale_slot() {
    let store = FakeAgentStore::new().with_pending(id("parent"), scope("earlier round"));
    let llm = FakeLlmClassifier::extracting(ExtractionOutcome::Unavailable);

    let outcome = stage_extracted_scope(&store, &llm, &id("parent"), "do the thing");

    assert!(matches!(
        outcome,
        StageExtractedScopeResult::ExtractionUnavailable(_)
    ));
    assert!(store.pending().borrow().is_empty());
}

#[test]
fn pending_write_failure_is_reported() {
    let store = FakeAgentStore::new().fail_pending_writes();
    let llm = extracting("do the thing");

    let outcome = stage_extracted_scope(&store, &llm, &id("parent"), "do the thing");

    assert!(matches!(
        outcome,
        StageExtractedScopeResult::StoreUnavailable(_)
    ));
}

#[test]
fn failure_to_clear_a_stale_slot_is_reported_as_store_unavailable() {
    let store = FakeAgentStore::new().fail_pending_clears();
    let llm = FakeLlmClassifier::extracting(ExtractionOutcome::NoScope);

    let outcome = stage_extracted_scope(&store, &llm, &id("parent"), "thanks!");

    assert!(matches!(
        outcome,
        StageExtractedScopeResult::StoreUnavailable(_)
    ));
}

#[test]
fn disabled_root_is_not_scoped_and_its_text_is_not_sent_to_the_llm() {
    let store = FakeAgentStore::new().with_disabled(id("root"));
    let llm = extracting("some task");

    let outcome = assign_extracted_scope(&store, &llm, &Agent::root(id("root")), "some task");

    assert!(matches!(outcome, AssignExtractedScopeResult::Disabled));
    assert!(store.scope_writes().borrow().is_empty());
    assert!(llm.extract_texts().borrow().is_empty());
}

#[test]
fn disable_read_failure_assigns_nothing_and_the_llm_is_not_called() {
    let store = FakeAgentStore::new().fail_disable_reads();
    let llm = extracting("some task");

    let outcome = assign_extracted_scope(&store, &llm, &Agent::root(id("root")), "some task");

    assert!(matches!(
        outcome,
        AssignExtractedScopeResult::StoreUnavailable(_)
    ));
    assert!(llm.extract_texts().borrow().is_empty());
}

#[test]
fn blank_text_assigns_nothing_and_the_llm_is_not_called() {
    let store = FakeAgentStore::new();
    let llm = extracting("some task");

    let outcome = assign_extracted_scope(&store, &llm, &Agent::root(id("root")), "  \n\t ");

    assert!(matches!(outcome, AssignExtractedScopeResult::NoScope));
    assert!(store.scope_writes().borrow().is_empty());
    assert!(llm.extract_texts().borrow().is_empty());
}

#[test]
fn blank_bundled_text_clears_a_stale_slot_without_calling_the_llm() {
    let store = FakeAgentStore::new().with_pending(id("parent"), scope("earlier round"));
    let llm = extracting("some task");

    let outcome = stage_extracted_scope(&store, &llm, &id("parent"), "");

    assert!(matches!(outcome, StageExtractedScopeResult::NoScope));
    assert!(store.pending().borrow().is_empty());
    assert!(llm.extract_texts().borrow().is_empty());
}
