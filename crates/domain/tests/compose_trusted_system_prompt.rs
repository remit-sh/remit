//! Tests for [`domain::compose_trusted_system_prompt`]. Pins the structure of
//! the trusted system-prompt block so adapters and #32's agent-template work
//! can rely on a stable assembly.

use domain::{
    AGENT_TEMPLATE_HEADER, CLASSIFIER_GUIDANCE, PROJECT_GUIDANCE_HEADER, StandingGuidance,
    compose_trusted_system_prompt,
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

#[test]
fn agent_template_slot_appends_header_then_tagged_body() {
    let standing = StandingGuidance {
        project: None,
        agent_template: Some("## Role\nArchitect.".into()),
    };
    let out = compose_trusted_system_prompt(&standing);
    let header = out.find(AGENT_TEMPLATE_HEADER).unwrap();
    // CLASSIFIER_GUIDANCE itself references `<agent_template>` as part of its
    // anti-injection framing — scan from the header onward to find the actual
    // tag that wraps the body.
    let tail = &out[header..];
    let open_tag = header + tail.find("<agent_template>").unwrap();
    let body = out.find("Architect.").unwrap();
    let close_tag = out.find("</agent_template>").unwrap();
    assert!(header < open_tag, "open tag must follow header");
    assert!(open_tag < body, "body must follow open tag");
    assert!(body < close_tag, "close tag must follow body");
}

#[test]
fn agent_template_body_is_emitted_verbatim_between_tags() {
    let body = "## Out of scope\n- Edit .rs files.";
    let standing = StandingGuidance {
        project: None,
        agent_template: Some(body.into()),
    };
    let out = compose_trusted_system_prompt(&standing);
    assert!(out.contains(&format!("<agent_template>\n{body}\n</agent_template>")));
}

#[test]
fn agent_template_none_does_not_emit_header_or_tags() {
    let standing = StandingGuidance::default();
    let out = compose_trusted_system_prompt(&standing);
    assert!(
        !out.contains(AGENT_TEMPLATE_HEADER),
        "header must not appear when agent_template is None"
    );
    // CLASSIFIER_GUIDANCE references `<agent_template>` for anti-injection
    // framing; the actual block we don't want is `HEADER\n<agent_template>`.
    assert!(
        !out.contains(&format!("{AGENT_TEMPLATE_HEADER}\n<agent_template>")),
        "tagged block must not appear when agent_template is None"
    );
}

#[test]
fn agent_template_block_follows_project_guidance_block() {
    // Slot order is fixed: <project_guidance> precedes <agent_template>.
    // Searches start past the AGENT_TEMPLATE_HEADER so we hit the actual tag,
    // not the CLASSIFIER_GUIDANCE anti-injection mention.
    let standing = StandingGuidance {
        project: Some("project body".into()),
        agent_template: Some("template body".into()),
    };
    let out = compose_trusted_system_prompt(&standing);
    let template_header = out.find(AGENT_TEMPLATE_HEADER).unwrap();
    let project_close = out.find("</project_guidance>").unwrap();
    let template_open = template_header + out[template_header..].find("<agent_template>").unwrap();
    assert!(
        project_close < template_open,
        "project guidance must precede agent template in the composed prompt"
    );
}

#[test]
fn agent_template_alone_omits_project_guidance_block() {
    let standing = StandingGuidance {
        project: None,
        agent_template: Some("body".into()),
    };
    let out = compose_trusted_system_prompt(&standing);
    // Match the framing tag (preceded by header+newline) rather than a bare
    // `\n<project_guidance>\n` substring; CLASSIFIER_GUIDANCE itself doesn't
    // emit that sequence so this is sufficient.
    assert!(!out.contains(&format!("{PROJECT_GUIDANCE_HEADER}\n<project_guidance>")));
    assert!(out.contains(&format!("{AGENT_TEMPLATE_HEADER}\n<agent_template>")));
}

#[test]
fn empty_agent_template_body_still_emits_block_with_empty_contents() {
    // Empty string is a deliberate authoring choice — operator wrote a 0-byte
    // file. Composer emits the block; the LLM sees an empty policy.
    let standing = StandingGuidance {
        project: None,
        agent_template: Some(String::new()),
    };
    let out = compose_trusted_system_prompt(&standing);
    assert!(out.contains("<agent_template>\n\n</agent_template>"));
}
