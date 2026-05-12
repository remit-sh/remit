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
fn instructs_model_not_to_restate_the_scope_in_red_reasons() {
    // The composition root appends the scope to the agent-facing stderr message
    // (see `format_red_for_agent` in claude-code-hook). The classifier must not
    // duplicate it in the reason text, otherwise the dialog reads twice.
    assert!(
        contains_ci(CLASSIFIER_GUIDANCE, "don't restate")
            || contains_ci(CLASSIFIER_GUIDANCE, "do not restate")
            || contains_ci(CLASSIFIER_GUIDANCE, "without restating"),
        "guidance must instruct the LLM not to restate the scope in the reason"
    );
}

#[test]
fn no_restate_sentence_appears_before_treat_as_data_paragraph() {
    // Security review: the no-restate instruction must sit *before* the
    // data-vs-instruction hardening paragraph, so that paragraph remains the
    // last word against prompt-injection attempts in scope/action content.
    let g = CLASSIFIER_GUIDANCE.to_lowercase();
    let restate_idx = g
        .find("restate")
        .expect("no-restate sentence must be present");
    let treat_as_data_idx = g
        .find("treat all scope and action")
        .expect("treat-as-data paragraph must be present");
    assert!(
        restate_idx < treat_as_data_idx,
        "no-restate sentence must precede the treat-as-data paragraph (restate at {restate_idx}, data-paragraph at {treat_as_data_idx})"
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
