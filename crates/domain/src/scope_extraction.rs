//! Scope acquisition by extraction: the LLM distils a [`Scope`] out of free
//! text and the result is bound through the [`AgentStore`]. Two callers, two
//! failure directions (TN-2026-017):
//!
//! - [`assign_extracted_scope`]: a root [`Agent`]'s Scope from user-authored
//!   text. Any non-`Assigned` outcome leaves the Agent unscoped, so the
//!   existing first-time dialog takes over.
//! - [`stage_extracted_scope`]: a **Bundled scope** from agent-authored spawn
//!   text. Any non-`Staged` outcome leaves nothing for the child to claim, so
//!   it is Red (`SubagentMissingScope`).

use crate::{Agent, AgentId, AgentStore, LlmClassifier, LlmClassifierError, ScopeStoreError};

/// Outcome of [`assign_extracted_scope`]. Only `Assigned` writes a Scope.
/// Vocab: [Extracted scope](../../../CONTEXT.md#agents).
#[derive(Debug)]
pub enum AssignExtractedScopeResult {
    Assigned,
    /// The Agent is a **Subagent**; its Scope is the parent's Bundled scope.
    NotRoot,
    /// The firewall is disabled for this Agent. The text is not sent out.
    Disabled,
    /// A Scope already exists. Never replaced automatically.
    AlreadyScoped,
    /// The text holds no Scope.
    NoScope,
    ExtractionUnavailable(LlmClassifierError),
    StoreUnavailable(ScopeStoreError),
}

/// Extract a [`crate::Scope`] from `text` and bind it to a root [`Agent`]
/// that has none. The LLM is consulted only for an enabled, unscoped root and
/// non-blank text. The Scope is re-read after the LLM call so one set
/// meanwhile (the dialog) wins.
pub fn assign_extracted_scope<S, C>(
    store: &S,
    llm: &C,
    agent: &Agent,
    text: &str,
) -> AssignExtractedScopeResult
where
    S: AgentStore,
    C: LlmClassifier,
{
    let Agent::Root { id } = agent else {
        return AssignExtractedScopeResult::NotRoot;
    };
    match store.is_disabled(id) {
        Ok(false) => {}
        Ok(true) => return AssignExtractedScopeResult::Disabled,
        Err(e) => return AssignExtractedScopeResult::StoreUnavailable(e),
    }
    match store.scope_for(id) {
        Ok(None) => {}
        Ok(Some(_)) => return AssignExtractedScopeResult::AlreadyScoped,
        Err(e) => return AssignExtractedScopeResult::StoreUnavailable(e),
    }
    if text.trim().is_empty() {
        return AssignExtractedScopeResult::NoScope;
    }
    let scope = match llm.extract_scope(text) {
        Ok(Some(scope)) => scope,
        Ok(None) => return AssignExtractedScopeResult::NoScope,
        Err(e) => return AssignExtractedScopeResult::ExtractionUnavailable(e),
    };
    match store.scope_for(id) {
        Ok(None) => {}
        Ok(Some(_)) => return AssignExtractedScopeResult::AlreadyScoped,
        Err(e) => return AssignExtractedScopeResult::StoreUnavailable(e),
    }
    match store.assign_scope(id, scope) {
        Ok(()) => AssignExtractedScopeResult::Assigned,
        Err(e) => AssignExtractedScopeResult::StoreUnavailable(e),
    }
}

/// Outcome of [`stage_extracted_scope`]. Only `Staged` leaves a pending row.
/// Vocab: [Bundled scope](../../../CONTEXT.md#agents).
#[derive(Debug)]
pub enum StageExtractedScopeResult {
    Staged,
    /// The text holds no Scope. Pending slot cleared.
    NoScope,
    /// Pending slot cleared.
    ExtractionUnavailable(LlmClassifierError),
    /// The pending slot could not be written or cleared; its state is
    /// unknown. Callers must not let the spawn proceed.
    StoreUnavailable(ScopeStoreError),
}

/// Extract a [`crate::Scope`] from a parent's spawn `text` and stage it under
/// `parent` for the child to claim. When nothing is staged the parent's
/// pending slot is cleared, so the child cannot claim a row left by an
/// earlier round. Blank text holds no Scope and is not sent to the LLM.
pub fn stage_extracted_scope<S, C>(
    store: &S,
    llm: &C,
    parent: &AgentId,
    text: &str,
) -> StageExtractedScopeResult
where
    S: AgentStore,
    C: LlmClassifier,
{
    let extracted = if text.trim().is_empty() {
        Ok(None)
    } else {
        llm.extract_scope(text)
    };
    let not_staged = match extracted {
        Ok(Some(scope)) => {
            return match store.set_pending(parent, scope) {
                Ok(()) => StageExtractedScopeResult::Staged,
                Err(e) => StageExtractedScopeResult::StoreUnavailable(e),
            };
        }
        Ok(None) => StageExtractedScopeResult::NoScope,
        Err(e) => StageExtractedScopeResult::ExtractionUnavailable(e),
    };
    match store.clear_pending(parent) {
        Ok(()) => not_staged,
        Err(e) => StageExtractedScopeResult::StoreUnavailable(e),
    }
}
