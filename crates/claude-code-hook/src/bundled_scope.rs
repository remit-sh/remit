//! The bundled-scope handoff between parent and child **Agent**.
//!
//! Parent's `PreToolUse` for a spawn-shaped tool stages the bundled
//! scope under the parent's id (`stage_pending`); the child's start
//! hook (`SubagentStart`, `SessionStart`) claims it and binds it as
//! its own scope (`claim_and_assign`). Both helpers are fail-closed:
//! they exit 2 on the store failures that would otherwise let the
//! child run unscoped.
//!
//! Stage and claim outcomes are recorded to the **Classification
//! history** via the generic `record_custom` API — the history writer
//! doesn't model bundled scope; the hook adapter owns the wire shape.

use domain::{AgentId, AgentStore, ClassificationHistoryWriter, Scope};
use file_scope_store::FileAgentStore;
use serde_json::json;

/// Outcome of staging a bundled scope. Scope text itself is not
/// persisted — bundled scope is potentially sensitive content per
/// CLAUDE.md, so the record stores only the staging outcome.
enum StagePendingOutcome<'a> {
    Set,
    Skipped { reason: &'a str },
    Failed { error: &'a str },
}

impl<'a> StagePendingOutcome<'a> {
    fn parts(&self) -> (&'static str, Option<&'a str>) {
        match self {
            StagePendingOutcome::Set => ("set", None),
            StagePendingOutcome::Skipped { reason } => ("skipped", Some(*reason)),
            StagePendingOutcome::Failed { error } => ("failed", Some(*error)),
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

fn record_stage_pending<W: ClassificationHistoryWriter>(
    writer: &W,
    parent: &AgentId,
    outcome: StagePendingOutcome<'_>,
) {
    let (outcome_label, detail) = outcome.parts();
    let mut payload = json!({
        "parent": parent.as_ref(),
        "outcome": outcome_label,
    });
    if let Some(d) = detail {
        payload["detail"] = json!(d);
    }
    writer.record_custom("stage_pending", payload);
}

fn record_claim_pending<W: ClassificationHistoryWriter>(
    writer: &W,
    hook: &str,
    parent: &AgentId,
    child: &AgentId,
    outcome: ClaimPendingOutcome<'_>,
) {
    let (outcome_label, detail) = outcome.parts();
    let mut payload = json!({
        "hook": hook,
        "parent": parent.as_ref(),
        "child": child.as_ref(),
        "outcome": outcome_label,
    });
    if let Some(d) = detail {
        payload["detail"] = json!(d);
    }
    writer.record_custom("claim_pending", payload);
}

/// Record the bundled scope under the parent's id, to be claimed by
/// the child's start hook. Empty-after-trim text is rejected at the
/// boundary; store failure exits 2 so the parent's spawn doesn't
/// proceed thinking it succeeded.
pub fn stage_pending<W: ClassificationHistoryWriter>(
    parent: &AgentId,
    scope_text: String,
    agent_store: &FileAgentStore,
    writer: &W,
) {
    let scope = match Scope::try_new(&scope_text) {
        Ok(s) => s,
        Err(_) => {
            // Fail-closed at the boundary: refuse to stage an empty/whitespace scope.
            // The child will hit `SubagentMissingScope` at first tool call, which is
            // strictly safer than binding a meaningless string.
            record_stage_pending(
                writer,
                parent,
                StagePendingOutcome::Skipped {
                    reason: "empty bundled scope",
                },
            );
            return;
        }
    };
    match agent_store.set_pending(parent, scope) {
        Ok(()) => record_stage_pending(writer, parent, StagePendingOutcome::Set),
        Err(e) => {
            // Fail-closed: deny the spawn so the parent doesn't proceed thinking
            // the scope was set.
            record_stage_pending(
                writer,
                parent,
                StagePendingOutcome::Failed {
                    error: &e.to_string(),
                },
            );
            std::process::exit(2);
        }
    }
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

    // Fail-closed signal: the pending row was consumed but the child's scope row
    // wasn't written. The child will hit `SubagentMissingScope` at first tool
    // call (verdict-correct) but the operator needs to see the underlying cause.
    if assign_failed_error.is_some() {
        eprintln!("remit: {hook} claim_and_assign: assign failed; child will run unscoped");
        std::process::exit(2);
    }
}
