use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, ClassificationInput, Classifier, CorrelationId,
    OverrideAction, PromptingDeps, RedKind, Scope, Verdict, VerdictChoice, process_action,
};

mod common;
use common::{
    FakeAgentStore, FakeGuidanceReader, FakeLlmClassifier, RecordingHistory, StubLock, StubPrompter,
};

fn id(s: &str) -> AgentId {
    AgentId::try_new(s).unwrap()
}

fn scope(s: &str) -> Scope {
    Scope::try_new(s).unwrap()
}

fn input(agent: Agent) -> ClassificationInput {
    ClassificationInput {
        agent,
        action: Action {
            call_id: None,
            kind: ActionKind::new("Bash"),
            args: ActionArgs::new("ls"),
        },
        working_dir: PathBuf::from("/tmp"),
        correlation_id: CorrelationId::new(),
    }
}

fn deps<'a>(
    prompter: &'a StubPrompter,
    lock: &'a StubLock,
) -> PromptingDeps<'a, StubPrompter, StubLock> {
    PromptingDeps { prompter, lock }
}

#[test]
fn green_when_existing_scope_present_and_llm_green() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("do the thing"));
    let classifier = Classifier::new(FakeLlmClassifier::green());
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new();
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    assert!(matches!(outcome.verdict, Verdict::Green));
    assert_eq!(*prompter.choose_calls().borrow(), 0);
    assert_eq!(*lock.call_count().borrow(), 0);
}

#[test]
fn green_when_session_disabled_skips_classification() {
    let store = FakeAgentStore::new()
        .with_scope(id("sess"), scope("do the thing"))
        .with_disabled(id("sess"));
    let llm = FakeLlmClassifier::red("would be red but bypassed");
    let llm_calls = llm.call_count();
    let classifier = Classifier::new(llm);
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new();
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    assert!(matches!(outcome.verdict, Verdict::Green));
    assert_eq!(
        *llm_calls.borrow(),
        0,
        "classifier must not run on disabled session"
    );
    assert!(
        history
            .state_transition_messages()
            .iter()
            .any(|e| e == "session-disabled"),
        "audit trail must mark the bypass"
    );
}

#[test]
fn red_subagent_skips_prompt_flow_entirely() {
    let store = FakeAgentStore::new();
    let classifier = Classifier::new(FakeLlmClassifier::green());
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new();
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::subagent(id("child"), id("parent"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let Verdict::Red { kind, .. } = outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::SubagentMissingScope);
    assert_eq!(*prompter.choose_calls().borrow(), 0);
    assert_eq!(*lock.call_count().borrow(), 0);
}

#[test]
fn no_scope_root_dispatches_to_update_scope_dialog() {
    let store = FakeAgentStore::new();
    let writes = store.scope_writes();
    let classifier = Classifier::new(FakeLlmClassifier::green().then_green());
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new().with_update_scope(scope("freshly typed"));
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("root"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    assert!(matches!(outcome.verdict, Verdict::Green));
    assert_eq!(writes.borrow().len(), 1, "scope was persisted");
    assert_eq!(
        *prompter.choose_calls().borrow(),
        0,
        "multi-button skipped on first-time setup"
    );
    assert_eq!(*prompter.update_calls().borrow(), 1);
}

#[test]
fn no_scope_root_user_cancels_setup_returns_scope_setup_cancelled() {
    let store = FakeAgentStore::new();
    let classifier = Classifier::new(FakeLlmClassifier::green());
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new().with_update_cancelled();
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("root"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let Verdict::Red { kind, .. } = outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::ScopeSetupCancelled);
}

#[test]
fn no_scope_root_update_unavailable_returns_scope_setup_unavailable() {
    let store = FakeAgentStore::new();
    let classifier = Classifier::new(FakeLlmClassifier::green());
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new().with_update_unavailable();
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("root"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let Verdict::Red { kind, .. } = outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::ScopeSetupUnavailable);
    assert!(
        history
            .state_transition_messages()
            .iter()
            .any(|e| e.starts_with("prompt-error: first-time")),
        "first-time prompt-error event recorded"
    );
}

#[test]
fn out_of_scope_user_picks_deny_returns_red() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("do the thing"));
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new().with_choose(VerdictChoice::Deny);
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let Verdict::Red { kind, .. } = outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::OutOfScope);
    assert_eq!(outcome.override_action, Some(OverrideAction::UserDeny));
}

#[test]
fn multi_button_prompt_carries_action_through_to_prompter() {
    // The post-Red dialog needs the Action so the adapter can render the
    // tool name + arg preview. Asserts `&input.action` reaches the prompter.
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("do the thing"));
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new().with_choose(VerdictChoice::Deny);
    let captured = prompter.last_choose_action();
    let lock = StubLock::new();
    let _ = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let captured = captured.borrow();
    let (kind, args) = captured.as_ref().expect("prompter.choose was called");
    assert_eq!(kind, "Bash");
    assert_eq!(args, "ls");
}

#[test]
fn out_of_scope_user_picks_allow_returns_green() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("do the thing"));
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new().with_choose(VerdictChoice::Allow);
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    assert!(matches!(outcome.verdict, Verdict::Green));
    // The initial verdict (pre-prompt) was Red; the final (post-prompt) is Green.
    assert!(matches!(outcome.initial_verdict, Some(Verdict::Red { .. })));
    assert_eq!(outcome.override_action, Some(OverrideAction::UserAllow));
}

#[test]
fn out_of_scope_user_picks_update_scope_then_re_classify_green() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("old scope"));
    let writes = store.scope_writes();
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant").then_green());
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new()
        .with_choose(VerdictChoice::UpdateScope)
        .with_update_scope(scope("new better scope"));
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    assert!(matches!(outcome.verdict, Verdict::Green));
    assert_eq!(writes.borrow().len(), 1);
    assert_eq!(writes.borrow()[0].1, scope("new better scope"));
}

#[test]
fn out_of_scope_user_cancels_edit_loops_back_to_multi_button() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("the scope"));
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new()
        .with_choose(VerdictChoice::UpdateScope)
        .with_update_cancelled()
        .with_choose(VerdictChoice::Deny);
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let Verdict::Red { .. } = outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(
        *prompter.choose_calls().borrow(),
        2,
        "two multi-button shows"
    );
    assert!(
        history
            .state_transition_messages()
            .iter()
            .any(|e| e == "user-cancelled-edit")
    );
}

#[test]
fn out_of_scope_user_picks_disable_session_writes_marker_and_returns_green() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("the scope"));
    let disabled = store.disable_writes();
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new()
        .with_choose(VerdictChoice::DisableSession)
        .with_confirm(true);
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    assert!(matches!(outcome.verdict, Verdict::Green));
    assert_eq!(
        outcome.override_action,
        Some(OverrideAction::DisableSession)
    );
    assert_eq!(disabled.borrow().len(), 1);
    assert_eq!(disabled.borrow()[0], id("sess"));
}

#[test]
fn out_of_scope_user_cancels_disable_confirm_loops_back() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("the scope"));
    let disabled = store.disable_writes();
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new()
        .with_choose(VerdictChoice::DisableSession)
        .with_confirm(false)
        .with_choose(VerdictChoice::Deny);
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let Verdict::Red { .. } = outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(*prompter.choose_calls().borrow(), 2);
    assert!(disabled.borrow().is_empty(), "marker not written on cancel");
}

#[test]
fn scope_changed_during_wait_re_classifies_with_new_scope_and_skips_prompt() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("old scope"));
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant").then_green());
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new();
    let scopes = store.scopes();
    let lock = StubLock::with_side_effect(move || {
        scopes.borrow_mut().insert(id("sess"), scope("new scope"));
    });
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    assert!(matches!(outcome.verdict, Verdict::Green));
    assert_eq!(
        *prompter.choose_calls().borrow(),
        0,
        "no dialog when re-classify with new scope yields Green"
    );
    assert!(
        history
            .state_transition_messages()
            .iter()
            .any(|e| e == "scope-changed-during-wait")
    );
}

#[test]
fn scope_changed_during_wait_still_red_proceeds_to_prompt() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("old scope"));
    let classifier = Classifier::new(FakeLlmClassifier::red("old reason").then_red("new reason"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new().with_choose(VerdictChoice::Allow);
    let scopes = store.scopes();
    let lock = StubLock::with_side_effect(move || {
        scopes.borrow_mut().insert(id("sess"), scope("new scope"));
    });
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    assert!(matches!(outcome.verdict, Verdict::Green));
    assert_eq!(
        *prompter.choose_calls().borrow(),
        1,
        "prompt fires when re-classify still Red"
    );
    let recorded = history.state_transition_messages();
    assert!(recorded.iter().any(|e| e == "scope-changed-during-wait"));
    assert!(recorded.iter().any(|e| e.starts_with("prompt-choose")));
}

#[test]
fn disabled_during_wait_returns_green_without_prompting() {
    let store = FakeAgentStore::new()
        .with_scope(id("sess"), scope("the scope"))
        .with_scripted_disable_reads(vec![false, true]);
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new();
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    assert!(matches!(outcome.verdict, Verdict::Green));
    assert_eq!(*prompter.choose_calls().borrow(), 0);
    assert!(
        history
            .state_transition_messages()
            .iter()
            .any(|e| e == "disabled-during-wait")
    );
}

#[test]
fn lock_unavailable_fails_closed() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("the scope"));
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new();
    let lock = StubLock::failing();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let Verdict::Red { kind, .. } = outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::ScopeSetupUnavailable);
}

#[test]
fn choose_unavailable_returns_original_red() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("the scope"));
    let classifier = Classifier::new(FakeLlmClassifier::red("the original reason"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new().with_choose_unavailable();
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let Verdict::Red { kind, reason } = outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::OutOfScope);
    assert_eq!(reason, "the original reason");
    assert!(
        history
            .state_transition_messages()
            .iter()
            .any(|e| e.starts_with("prompt-error: multi-button")),
        "prompt-error event recorded"
    );
}

#[test]
fn store_unavailable_when_reader_fails_initially() {
    let store = FakeAgentStore::new().fail_scope_reads();
    let classifier = Classifier::new(FakeLlmClassifier::green());
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new();
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("root"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let Verdict::Red { kind, .. } = outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::StoreUnavailable);
}

#[test]
fn disable_reader_failure_fails_closed() {
    let store = FakeAgentStore::new()
        .with_scope(id("sess"), scope("the scope"))
        .fail_disable_reads();
    let classifier = Classifier::new(FakeLlmClassifier::green());
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new();
    let lock = StubLock::new();
    let outcome = process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let Verdict::Red { kind, .. } = outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::StoreUnavailable);
}

#[test]
fn logs_audit_events_in_expected_sequence_for_allow_path() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("the scope"));
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new().with_choose(VerdictChoice::Allow);
    let lock = StubLock::new();
    process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let recorded = history.state_transition_messages();
    assert_eq!(recorded.len(), 3, "exactly three events on the Allow path");
    assert_eq!(recorded[0], "prompt-lock-acquired");
    assert!(recorded[1].starts_with("prompt-choose: kind="));
    assert_eq!(recorded[2], "user-allow");
}

#[test]
fn logs_audit_events_in_expected_sequence_for_disable_session_path() {
    let store = FakeAgentStore::new().with_scope(id("sess"), scope("the scope"));
    let classifier = Classifier::new(FakeLlmClassifier::red("not relevant"));
    let history = RecordingHistory::new();
    let prompter = StubPrompter::new()
        .with_choose(VerdictChoice::DisableSession)
        .with_confirm(true);
    let lock = StubLock::new();
    process_action(
        &input(Agent::root(id("sess"))),
        &store,
        &classifier,
        &history,
        &deps(&prompter, &lock),
        &FakeGuidanceReader::none(),
        false,
    );
    let recorded = history.state_transition_messages();
    assert_eq!(
        recorded,
        vec![
            "prompt-lock-acquired".to_string(),
            "prompt-choose: kind=OutOfScope".to_string(),
            "prompt-confirm-disable".to_string(),
            "user-confirmed-disable".to_string(),
        ]
    );
}
