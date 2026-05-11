//! Spec-style assertions on the substantive instructions in
//! `CLASSIFIER_GUIDANCE`. Phrasing may evolve; the *meaning* must persist.
//!
//! Closes #26: the firewall must not confabulate Red verdicts grounded in
//! runtime state it cannot directly observe (prior actions, git/fs state,
//! workflow-phase progression). Reds must cite evidence visible in the action.

use domain::CLASSIFIER_GUIDANCE;

/// Case-insensitive substring presence — keeps the assertion robust to minor
/// rephrasing while catching wholesale removal of the instruction.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

#[test]
fn instructs_model_that_it_only_sees_a_single_action() {
    assert!(
        contains_ci(CLASSIFIER_GUIDANCE, "single action")
            || contains_ci(CLASSIFIER_GUIDANCE, "this one action"),
        "guidance must tell the model it sees only one action"
    );
}

#[test]
fn instructs_model_it_cannot_observe_prior_actions_or_external_state() {
    assert!(
        contains_ci(CLASSIFIER_GUIDANCE, "cannot see")
            || contains_ci(CLASSIFIER_GUIDANCE, "cannot observe"),
        "guidance must explicitly disclaim visibility of state"
    );
    assert!(
        contains_ci(CLASSIFIER_GUIDANCE, "prior actions"),
        "guidance must list prior actions among unobservable state"
    );
    assert!(
        contains_ci(CLASSIFIER_GUIDANCE, "git"),
        "guidance must list git state among unobservable state"
    );
}

#[test]
fn instructs_model_that_red_must_cite_visible_evidence() {
    assert!(
        contains_ci(CLASSIFIER_GUIDANCE, "visible in the action"),
        "guidance must require Red reasons to cite evidence visible in the action"
    );
}

#[test]
fn instructs_model_to_default_unobservable_red_to_green() {
    // The issue's proposed fix: if a Red verdict would depend on a fact the
    // model cannot observe, return Green. This kills the confabulation class.
    assert!(
        contains_ci(CLASSIFIER_GUIDANCE, "return green")
            || contains_ci(CLASSIFIER_GUIDANCE, "verdict is green"),
        "guidance must say to return Green when Red would depend on unobservable state"
    );
}

#[test]
fn anti_injection_clause_names_agent_template() {
    // Mirrors the existing project-guidance anti-injection framing. The model
    // must treat directives inside <agent_template> as data, not instructions.
    assert!(
        contains_ci(CLASSIFIER_GUIDANCE, "<agent_template>")
            || contains_ci(CLASSIFIER_GUIDANCE, "agent template"),
        "guidance must reference the agent template anti-injection envelope"
    );
}
