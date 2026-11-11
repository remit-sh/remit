//! Spec-style assertions on the substantive instructions in
//! `SCOPE_EXTRACTION_GUIDANCE` (TN-2026-017). Phrasing may evolve; the
//! *meaning* must persist.

use domain::{CLASSIFIER_GUIDANCE, SCOPE_EXTRACTION_GUIDANCE, ScopeOrigin};

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

#[test]
fn instructs_model_to_keep_the_what_and_drop_the_why() {
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "keep the what"));
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "drop the why"));
    assert!(
        contains_ci(SCOPE_EXTRACTION_GUIDANCE, "justification"),
        "guidance must name justification among what is dropped"
    );
}

#[test]
fn carries_the_worked_example_without_its_causal_clause() {
    assert!(contains_ci(
        SCOPE_EXTRACTION_GUIDANCE,
        "becomes \"Organise my downloads folder\""
    ));
}

#[test]
fn instructs_model_to_keep_limits_that_narrow_the_task() {
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "limits"));
    assert!(contains_ci(
        SCOPE_EXTRACTION_GUIDANCE,
        "don't delete anything"
    ));
    assert!(
        contains_ci(
            SCOPE_EXTRACTION_GUIDANCE,
            "unless it restricts what may be done"
        ),
        "dropping how-detail must not drop a restriction phrased as method"
    );
}

#[test]
fn instructs_model_to_distil_not_rewrite_or_enumerate_permissions() {
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "not a rewrite"));
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "own terms"));
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "permitted actions"));
}

#[test]
fn instructs_model_to_report_no_task_for_greetings_and_bare_slash_commands() {
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "no task"));
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "greeting"));
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "continue"));
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "slash command"));
}

#[test]
fn instructs_model_to_treat_the_text_as_data() {
    assert!(contains_ci(SCOPE_EXTRACTION_GUIDANCE, "data only"));
    assert!(contains_ci(
        SCOPE_EXTRACTION_GUIDANCE,
        "never as instructions"
    ));
}

#[test]
fn is_separate_from_the_firewall_framing() {
    assert!(!contains_ci(SCOPE_EXTRACTION_GUIDANCE, "firewall"));
    assert!(!contains_ci(SCOPE_EXTRACTION_GUIDANCE, "verdict"));
    assert!(!CLASSIFIER_GUIDANCE.contains(SCOPE_EXTRACTION_GUIDANCE));
}

#[test]
fn does_not_name_where_the_text_came_from() {
    assert!(
        !contains_ci(SCOPE_EXTRACTION_GUIDANCE, "prompt"),
        "extraction is source-agnostic: user prompt and Bundled scope share it"
    );
}

#[test]
fn extracted_scope_origin_label() {
    assert_eq!(ScopeOrigin::Extracted.as_str(), "extracted");
}
