//! `UserPromptSubmit`: a root **Agent** with no **Scope** gets one extracted
//! from the user's prompt (TN-2026-017).
//!
//! This event carries no verdict, and exit 2 would block the user's prompt.
//! Every failure therefore leaves the Agent unscoped and returns normally:
//! the first-time dialog (or Red) at the first tool call is the fallback.
//! Nothing is written to stdout, which Claude Code would inject as context.

use std::collections::BTreeMap;

use domain::{
    AgentId, AssignExtractedScopeResult, ClassificationEvent, ClassificationHistoryWriter,
    CorrelationId, LlmClassifierError, ScopeOrigin, assign_extracted_scope,
};
use file_classification_history::FileClassificationHistoryWriter;
use file_scope_store::FileAgentStore;

use crate::{
    agent_from, lifecycle_event, parent_from_env, truncate_for_details, try_build_llm_classifier,
    warn_on_history_err,
};

#[derive(serde::Deserialize)]
pub(crate) struct UserPromptSubmitPayload {
    session_id: String,
    /// Not sent for user prompts today; if it ever is, the prompt belongs to
    /// an in-process subagent and must not set the root's scope.
    agent_id: Option<String>,
    prompt: String,
}

/// Run the handler. `Err` says why setup failed before an outcome could be
/// recorded: fixed text only, never payload or config content.
pub(crate) fn handle_user_prompt_submit(
    raw_payload: serde_json::Value,
    correlation_id: &CorrelationId,
) -> Result<(), &'static str> {
    let cfg = remit_config::load().map_err(|_| "config unavailable")?;
    let agents_dir = remit_config::agents_dir().map_err(|_| "agents dir unavailable")?;
    let log_path = remit_config::log_path().map_err(|_| "log path unavailable")?;
    let writer = FileClassificationHistoryWriter::new(log_path);
    warn_on_history_err(writer.write_event(lifecycle_event(
        "hook-event: UserPromptSubmit".into(),
        BTreeMap::new(),
        Some(correlation_id),
        cfg.dev_bypass,
    )));

    let payload: UserPromptSubmitPayload =
        serde_json::from_value(raw_payload).map_err(|_| "payload: unexpected shape")?;
    let session =
        AgentId::try_new(payload.session_id).map_err(|_| "payload: invalid session id")?;
    let agent_id = payload
        .agent_id
        .map(AgentId::try_new)
        .transpose()
        .map_err(|_| "payload: invalid agent id")?;
    let agent = agent_from(session, agent_id, parent_from_env);

    let outcome = match try_build_llm_classifier(&cfg) {
        Ok(llm) => {
            let agent_store = FileAgentStore::new(agents_dir);
            assign_extracted_scope(&agent_store, &llm, &agent, &payload.prompt)
        }
        Err(e) => AssignExtractedScopeResult::ExtractionUnavailable(
            LlmClassifierError::Unavailable(Box::new(e)),
        ),
    };

    let (label, detail) = outcome_parts(&outcome);
    let mut details = BTreeMap::new();
    details.insert("outcome".into(), label.into());
    if let Some(detail) = detail {
        details.insert("detail".into(), truncate_for_details(&detail));
    }
    if matches!(outcome, AssignExtractedScopeResult::Assigned) {
        details.insert(
            "scope_origin".into(),
            ScopeOrigin::Extracted.as_str().into(),
        );
    }
    warn_on_history_err(writer.write_event(ClassificationEvent {
        session_id: Some(agent.id().clone()),
        ..lifecycle_event(
            format!("extract_scope: {label}"),
            details,
            Some(correlation_id),
            cfg.dev_bypass,
        )
    }));
    Ok(())
}

/// Audit label + optional error detail for an outcome. Scope text is never
/// part of either.
pub(crate) fn outcome_parts(
    outcome: &AssignExtractedScopeResult,
) -> (&'static str, Option<String>) {
    match outcome {
        AssignExtractedScopeResult::Assigned => ("assigned", None),
        AssignExtractedScopeResult::NotRoot => ("not_root", None),
        AssignExtractedScopeResult::Disabled => ("disabled", None),
        AssignExtractedScopeResult::AlreadyScoped => ("already_scoped", None),
        AssignExtractedScopeResult::NoScope => ("no_scope", None),
        AssignExtractedScopeResult::ExtractionUnavailable(e) => {
            ("extraction_unavailable", Some(e.to_string()))
        }
        AssignExtractedScopeResult::StoreUnavailable(e) => {
            ("store_unavailable", Some(e.to_string()))
        }
    }
}
