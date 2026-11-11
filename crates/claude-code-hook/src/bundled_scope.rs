//! The bundled-scope handoff between parent and child **Agent**.
//!
//! Parent's `PreToolUse` for a spawn-shaped tool distils the bundled
//! text into a scope and stages it under the parent's id
//! (`stage_pending`); the child's start hook (`SubagentStart`,
//! `SessionStart`) claims it and binds it as its own scope
//! (`claim_and_assign`). Both helpers are fail-closed:
//! they exit 2 on the store failures that would otherwise let the
//! child run unscoped.
//!
//! Stage and claim outcomes are recorded via `write_event` with
//! `EventKind::Lifecycle` — the history writer doesn't model bundled
//! scope; the hook adapter owns the wire shape via `details` map.

use std::collections::BTreeMap;

use domain::{
    AgentId, AgentStore, ClassificationEvent, ClassificationHistoryWriter, EventKind,
    LlmClassifier, StageExtractedScopeResult, stage_extracted_scope,
};
use file_scope_store::FileAgentStore;

use crate::truncate_for_details;

/// Audit label + optional detail for a staging outcome. Scope text itself
/// is not persisted — bundled scope is potentially sensitive content per
/// CLAUDE.md, so the record stores only the staging outcome.
pub(crate) fn stage_outcome_parts(
    outcome: &StageExtractedScopeResult,
) -> (&'static str, Option<String>) {
    match outcome {
        StageExtractedScopeResult::Staged => ("staged", None),
        StageExtractedScopeResult::NoScope => ("no_scope", None),
        StageExtractedScopeResult::ExtractionUnavailable(e) => {
            ("extraction_unavailable", Some(e.to_string()))
        }
        StageExtractedScopeResult::StoreUnavailable(e) => {
            ("store_unavailable", Some(e.to_string()))
        }
    }
}

/// Outcome of claiming a parent's staged bundled scope on the child's
/// start hook. Scope text itself is not persisted.
enum ClaimPendingOutcome<'a> {
    Hit,
    HitAssignFailed { error: &'a str },
    Miss,
    Error { error: &'a str },
}

impl<'a> ClaimPendingOutcome<'a> {
    fn parts(&self) -> (&'static str, Option<&'a str>) {
        match self {
            ClaimPendingOutcome::Hit => ("hit", None),
            ClaimPendingOutcome::HitAssignFailed { error } => ("hit_assign_failed", Some(*error)),
            ClaimPendingOutcome::Miss => ("miss", None),
            ClaimPendingOutcome::Error { error } => ("error", Some(*error)),
        }
    }
}

fn lifecycle_event(message: String, details: BTreeMap<String, String>) -> ClassificationEvent {
    ClassificationEvent {
        ts: time::OffsetDateTime::now_utc(),
        session_id: None,
        correlation_id: None,
        kind: EventKind::Lifecycle,
        message,
        details,
        dev_bypass: false,
    }
}

fn record_stage_pending<W: ClassificationHistoryWriter>(
    writer: &W,
    parent: &AgentId,
    outcome: &StageExtractedScopeResult,
) {
    let (outcome_label, detail) = stage_outcome_parts(outcome);
    let mut details = BTreeMap::new();
    details.insert("parent".into(), parent.as_ref().to_string());
    details.insert("outcome".into(), outcome_label.into());
    if let Some(d) = detail {
        details.insert("detail".into(), truncate_for_details(&d));
    }
    let _ = writer.write_event(lifecycle_event(
        format!("stage_pending: {outcome_label}"),
        details,
    ));
}

fn record_claim_pending<W: ClassificationHistoryWriter>(
    writer: &W,
    hook: &str,
    parent: &AgentId,
    child: &AgentId,
    outcome: ClaimPendingOutcome<'_>,
) {
    let (outcome_label, detail) = outcome.parts();
    let mut details = BTreeMap::new();
    details.insert("hook".into(), hook.into());
    details.insert("parent".into(), parent.as_ref().to_string());
    details.insert("child".into(), child.as_ref().to_string());
    details.insert("outcome".into(), outcome_label.into());
    if let Some(d) = detail {
        details.insert("detail".into(), d.into());
    }
    let _ = writer.write_event(lifecycle_event(
        format!("claim_pending: {outcome_label}"),
        details,
    ));
}

/// Distil the bundled text into a scope and record it under the parent's
/// id, to be claimed by the child's start hook. Extraction failure or text
/// holding no scope stages nothing: the child runs unscoped, so its first
/// tool call is Red. Store failure exits 2 so the parent's spawn doesn't
/// proceed with the pending slot in an unknown state.
pub fn stage_pending<C: LlmClassifier, W: ClassificationHistoryWriter>(
    parent: &AgentId,
    bundled_text: &str,
    llm: &C,
    agent_store: &FileAgentStore,
    writer: &W,
) {
    let outcome = stage_extracted_scope(agent_store, llm, parent, bundled_text);
    record_stage_pending(writer, parent, &outcome);
    if matches!(outcome, StageExtractedScopeResult::StoreUnavailable(_)) {
        eprintln!("remit: stage bundled scope: agent store unavailable; deny");
        std::process::exit(2);
    }
}

/// Drop whatever is staged under `parent`: the spawn it was staged for was
/// denied. Best-effort, since the caller is already denying the action.
pub fn discard_pending<W: ClassificationHistoryWriter>(
    parent: &AgentId,
    agent_store: &FileAgentStore,
    writer: &W,
) {
    let outcome_label = match agent_store.clear_pending(parent) {
        Ok(()) => "discarded",
        Err(_) => "discard_failed",
    };
    let mut details = BTreeMap::new();
    details.insert("parent".into(), parent.as_ref().to_string());
    details.insert("outcome".into(), outcome_label.into());
    let _ = writer.write_event(lifecycle_event(
        format!("stage_pending: {outcome_label}"),
        details,
    ));
}

/// Claim the parent's pending row and bind the scope to the child's
/// id. Hit → `assign_scope` under the child; miss → child will be
/// blocked at first tool call. Exits 2 if assign fails after
/// `take_pending` consumed the row.
pub fn claim_and_assign<W: ClassificationHistoryWriter>(
    agent_store: &FileAgentStore,
    parent: &AgentId,
    child: &AgentId,
    hook: &str,
    writer: &W,
) {
    let claim = agent_store.take_pending(parent);
    let assign_failed_error = match &claim {
        Ok(Some(scope)) => match agent_store.assign_scope(child, scope.clone()) {
            Ok(()) => {
                record_claim_pending(writer, hook, parent, child, ClaimPendingOutcome::Hit);
                None
            }
            Err(e) => {
                let msg = e.to_string();
                record_claim_pending(
                    writer,
                    hook,
                    parent,
                    child,
                    ClaimPendingOutcome::HitAssignFailed { error: &msg },
                );
                Some(msg)
            }
        },
        Ok(None) => {
            record_claim_pending(writer, hook, parent, child, ClaimPendingOutcome::Miss);
            None
        }
        Err(e) => {
            record_claim_pending(
                writer,
                hook,
                parent,
                child,
                ClaimPendingOutcome::Error {
                    error: &e.to_string(),
                },
            );
            None
        }
    };

    if assign_failed_error.is_some() {
        eprintln!("remit: {hook} claim_and_assign: assign failed; child will run unscoped");
        std::process::exit(2);
    }
}
