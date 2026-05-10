//! Tests for [`domain::compose_trusted_system_prompt`]. Pins the structure of
//! the trusted system-prompt block so adapters and #32's agent-template work
//! can rely on a stable assembly.

use domain::{
    CLASSIFIER_GUIDANCE, PROJECT_GUIDANCE_HEADER, StandingGuidance, compose_trusted_system_prompt,
};

#[test]
fn empty_standing_guidance_yields_classifier_guidance_alone() {
    let out = compose_trusted_system_prompt(&StandingGuidance::default());
    assert_eq!(out, CLASSIFIER_GUIDANCE);
}

#[test]
fn project_slot_appends_header_then_tagged_body_after_classifier_guidance() {
    let standing = StandingGuidance {
        project: Some("## Project\nRust CLI.".into()),
        agent_template: None,
    };
    let out = compose_trusted_system_prompt(&standing);
    let cg = out.find(CLASSIFIER_GUIDANCE).unwrap();
    let header = out.find(PROJECT_GUIDANCE_HEADER).unwrap();
    // CLASSIFIER_GUIDANCE itself references `<project_guidance>` as part of its
    // anti-injection framing — scan from the header onward to find the actual
    // tag that wraps the body.
    let tail = &out[header..];
    let open_tag = header + tail.find("<project_guidance>").unwrap();
    let body = out.find("Rust CLI.").unwrap();
    let close_tag = out.find("</project_guidance>").unwrap();
    assert!(cg < header, "header must follow CLASSIFIER_GUIDANCE");
    assert!(header < open_tag, "open tag must follow header");
    assert!(open_tag < body, "body must follow open tag");
    assert!(body < close_tag, "close tag must follow body");
}

#[test]
fn project_body_is_emitted_verbatim_between_tags() {
    let body = "## Tooling\n- `just check`\n- `cargo test --workspace`";
    let standing = StandingGuidance {
        project: Some(body.into()),
        agent_template: None,
    };
    let out = compose_trusted_system_prompt(&standing);
    assert!(out.contains(&format!("<project_guidance>\n{body}\n</project_guidance>")));
}

#[test]
fn project_none_does_not_emit_header_or_tags() {
    let standing = StandingGuidance::default();
    let out = compose_trusted_system_prompt(&standing);
    assert!(
        !out.contains(PROJECT_GUIDANCE_HEADER),
        "header must not appear when project is None"
    );
    assert!(
        !out.contains("\n<project_guidance>\n"),
        "tagged block must not appear when project is None"
    );
}

#[test]
fn classifier_guidance_appears_first_and_unchanged() {
    let standing = StandingGuidance {
        project: Some("x".into()),
        agent_template: None,
    };
    let out = compose_trusted_system_prompt(&standing);
    assert!(out.starts_with(CLASSIFIER_GUIDANCE));
}
