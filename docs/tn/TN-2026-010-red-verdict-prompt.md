---
number: 2026-010
title: User-bypass prompt on Red verdicts
kind: plan
status: active
issue: "#21"
tags: [classifier, prompt, ux, concurrency, fail-closed]
---

# TN-2026-010: User-bypass prompt on Red verdicts

> On Red, the root agent's user gets a dialog with four choices: Deny, Allow, Update scope, Disable for session. Parallel Reds serialise behind a per-session lock and re-evaluate before showing — Update scope and Disable session cascade naturally; Allow and Deny remain per-call.

## Problem

- Today, Red exits 2 silently. The user has no recourse short of editing config or restarting.
- Many Reds are fixable in-the-moment: a stale scope, a one-off action that's fine, or an "I'm done with the firewall today" intent.

## Decision

A single unified `Prompter` covers all user-facing dialogs (first-time scope setup, post-Red choice, scope update, disable confirm). Subagents continue to exit 2 silently regardless of cause; only **root agents** prompt.

The existing `ScopePrompter` trait is removed; `acquire_scope` no longer prompts. The classifier emits `RedKind::NoScopeRoot` as today; the post-Red flow dispatches on `RedKind`:

- `NoScopeRoot` → straight to **update-scope** text input (current=`None`). On save → re-classify. On cancel → final Verdict::Red (`ScopeSetupCancelled`).
- Any other Red → multi-button dialog. From there the user can update the scope (re-uses the same text input), Allow once, Deny, or Disable for session.

Multi-button choices:

| Button | Effect |
|---|---|
| **Deny** *(default)* | Exit 2. Current behaviour. |
| **Allow** | Exit 0 for this call only. |
| **Update scope** | Show update-scope text input pre-filled with the current scope; on save, persist via `AgentStore::assign_scope` and re-classify. On cancel, return to the multi-button dialog. |
| **Disable session** | Show "Are you sure?" confirm; on yes, write a marker keyed by `session_id`; subsequent hooks under that id skip classification entirely (Green). |

Reuse the `osascript` adapter — `display dialog` covers multi-button choice and text input both.

## Components

### Domain (new ports + types)

- `enum VerdictChoice { Deny, Allow, UpdateScope, DisableSession }`.
- `trait Prompter { fn choose(scope, reason, cwd) -> Result<VerdictChoice, PromptError>; fn update_scope(current: Option<&Scope>) -> Result<Scope, PromptError>; fn confirm_disable() -> Result<bool, PromptError>; }` — replaces `ScopePrompter`. `update_scope(None)` is first-time setup (empty default); `update_scope(Some(...))` pre-fills the current scope.
- Existing `ScopePrompter` trait deleted. `acquire_scope` becomes read-only: rename to `resolve_scope` (already exists as a non-prompting helper) and inline; remove the prompter parameter from `process_action`.
- `trait AgentStore` — single aggregate port for all [`Agent`]-keyed Remit state, replacing `ScopeReader` / `ScopeWriter` / `PendingScopeStore` / `SessionDisableReader` / `SessionDisableWriter`. Methods cluster into three concerns:
  - **Scope:** `scope_for(agent_id)` / `assign_scope(agent_id, scope)`.
  - **Bundled-scope staging:** `set_pending(parent, scope)` / `take_pending(parent)`.
  - **Disable marker:** `is_disabled(agent_id)` / `disable(agent_id)`.
  All keyed by [`AgentId`]; disable methods are conventionally called with the root agent's id (via `root_key(agent)`) but the trait doesn't enforce that distinction. Vocab: avoids "Session" in domain code per `UBIQUITOUS_LANGUAGE.md` ("Session" is Claude-Code-specific; the domain term is **Agent**).
- `trait PromptLock { type Guard; fn acquire(&self, agent_id: &AgentId) -> Result<Self::Guard, PromptError>; }`. Blocking acquisition; queue serialises behind any in-flight prompt for the same root. The associated type keeps the file handle in the adapter; domain holds an opaque value whose only contract is "drop releases the lock." Kept separate from `AgentStore` because RAII guard semantics don't fit a CRUD-style "store" model.
- `struct PromptingDeps<'a, P: Prompter, K: PromptLock> { prompter: &'a P, lock: &'a K }` — bundles the prompting-subsystem ports so `process_action`'s signature stays narrow. Lives in domain; adapters know nothing about it.
- Extend `ClassifierLogger` with `fn log_event(&self, input: &ClassificationInput, event: &str)` for audit trail of state transitions inside `process_action` (lock waits, user choices, scope updates, disable confirms). See the **Audit logging** section below for the message catalogue.

### `process_action` flow update

`process_action` is the orchestrator. The post-Red prompting work lives in `run_owner_prompt_loop`, called only after the lock is held and the verdict has been re-evaluated. No `Owner` / `Cascade` distinction — every queued hook simply re-evaluates against fresh state when it gets the lock; if state changed (scope updated, session disabled), the prompt is skipped.

```
process_action:
  1. logger.log_incoming
  2. if store.is_disabled(root_key(agent)) → log_outcome(Green); return Green
  3. let scope_initial = resolve_scope(store, agent)
  4. let initial = classifier.classify(input, scope_initial)
  5. if Green → log_outcome; return
  6. if Red and agent.has_parent() → log_outcome; return Red
  // root + Red — enter prompt flow (lock held until step 13 returns)
  7. let _guard = lock.acquire(root_key(agent))?       // blocks until our turn
     log_event("prompt-lock-acquired")
  8. if store.is_disabled(root_key(agent)) → log_event("disabled-during-wait"); log_outcome(Green); return Green
  9. let scope_now = resolve_scope(store, agent)
 10. let current_red = if scope_now == scope_initial {
       initial            // cached; no second LLM call
     } else {
       log_event("scope-changed-during-wait")
       classifier.classify(input, scope_now)
     }
 11. if current_red is Green → log_outcome(Green); return Green
 12. let final = run_owner_prompt_loop(input, current_red, ...)
 13. log_outcome(final); return final            // _guard drops, lock released
```

```
run_owner_prompt_loop(input, current_red, ...):
  loop:
    match current_red.kind:
      NoScopeRoot →
        log_event("prompt-update-scope: first-time")
        match prompter.update_scope(None):
          Ok(scope) → log_event("user-typed-scope"); store.assign_scope; current_red = re-classify; if Green return Green
          Err(Cancelled) → log_event("user-cancelled-scope-setup"); return Verdict::Red { kind: ScopeSetupCancelled }
          Err(Unavailable) → log_error; return Verdict::Red { kind: ScopeSetupUnavailable }
      _ (OutOfScope / ClassifierUnavailable / ...) →
        log_event("prompt-choose: kind=...")
        match prompter.choose(scope, current_red.reason, cwd):
          Deny | Err(Cancelled) → log_event("user-deny"); return current_red
          Err(Unavailable) → log_error; return current_red
          Allow → log_event("user-allow"); return Verdict::Green
          UpdateScope →
            log_event("prompt-update-scope: edit")
            match prompter.update_scope(Some(scope)):
              Ok(new) → log_event("user-typed-scope"); store.assign_scope; current_red = re-classify; if Green return Green
              Err(Cancelled) → log_event("user-cancelled-edit"); continue loop
              Err(Unavailable) → log_error; continue loop
          DisableSession →
            log_event("prompt-confirm-disable")
            match prompter.confirm_disable():
              Ok(true) → log_event("user-confirmed-disable"); store.disable(root_key(agent)); return Verdict::Green
              Ok(false) → log_event("user-cancelled-disable"); continue loop
              Err(_) → log_error; continue loop
```

`root_key(agent)` = `agent.id` for root, `agent.parent_id` for subagent. One-level walk only — covers in-process and peer-process subagents (whose parent_id is set from env).

Each iteration of the owner loop re-renders the dialog with the **current** Red's reason — if `UpdateScope` produced a new kind/reason, the user sees that on the next iteration.

The loop lives in domain (not the composition root) so adapter tests don't need to drive UI; domain tests hit the flow with stub prompters.

**Re-evaluation rationale**: queued hooks observe whatever side effects the previous owner committed (new scope via `AgentStore::assign_scope`, disable marker via `AgentStore::disable`) by re-checking disable + re-resolving scope after acquiring the lock. If scope changed, re-classify. The store IS the cascade channel — no enum, no outcome file. Each Red still gets its own decision unless the state change resolves it.

### Adapter — `osascript-prompter`

Replace the `ScopePrompter` impl with a single `Prompter` impl. Three osascript invocations:

- **`choose`** — `display dialog "<scope/reason/cwd block>" buttons {"Deny", "Allow", "Update scope", "Disable session"} default button "Deny"`. Parse `button returned:`.
- **`update_scope`** — `display dialog "Update scope" default answer "<current scope or empty>" buttons {"Cancel", "Save"} default button "Save"`. Reuse the existing `text returned:` parser. Empty / whitespace-only answer → `PromptError::Cancelled`.
- **`confirm_disable`** — `display dialog "Disable Remit for the rest of this session?" buttons {"Cancel", "Disable"} default button "Cancel"`. Map button to `bool`.

Adapter only — wire formatting, parsing, error mapping. No business logic.

### Adapter — `file-scope-store`

Replace the three pre-existing structs (`FileScopeStore`, `FilePendingScopeStore`, `FileSessionDisableStore`) with one `FileAgentStore` impl `AgentStore`. Constructor takes one root directory; per-agent subdirectories hold a small fixed set of files:

```
<agents_dir>/<agent-id>/
  scope.json        — bound scope (any agent)
  pending.json      — bundled scope staged for next child (parent only)
  disabled.marker   — disable flag (root only; existence = disabled)
  prompt.lock       — per-root flock (owned by FsPromptLock, same dir tree)
```

Per-agent dirs are created lazily on first write. Read errors that aren't `NotFound` map to `ScopeStoreError::Unavailable` → upstream Red, fail-closed.

### `remit-config`

- One helper: `agents_dir()` returns `<remit_home>/agents/`. Both `FileAgentStore` and `FsPromptLock` take the same path — all agent-keyed state lives under this single tree.

### Adapter — `prompt-lock` (new crate)

New crate name follows the by-role rule from CLAUDE.md (existing crates `file-logger` / `file-scope-store` / `osascript-prompter` are technology-prefixed drift to be cleaned up separately, not extended here).

`FsPromptLock` impls `PromptLock`. Pattern mirrors `FileLogger::log_line` (TN-2026-006).

- Per-root lock file: `<agents_dir>/<agent-id>/prompt.lock` (same dir tree as `FileAgentStore`).
- `acquire(agent_id)`:
  1. `OpenOptions::create(true).write(true).open(path)`.
  2. `file.lock()` — blocking; queues behind any in-flight owner of the same root's lock.
  3. Return a guard struct holding `file`; `Drop` releases.
- IO failure → `PromptError::Unavailable`. Composition root maps to fail-closed Red.

Per-agent directory is created lazily; never auto-cleaned (markers + lock files accumulate under `<agents_dir>/`; v1 leaves cleanup to a follow-up).

### Composition root — `claude-code-hook`

- Construct `OsascriptPrompter` (covers all three dialog calls), `FileAgentStore` (the consolidated store wired with three subdirs from `remit-config`), `FsPromptLock`.
- Bundle prompter + lock into a `PromptingDeps` value.
- Pass `agent_store` + `prompting` to `process_action` alongside the classifier and logger.

## Concurrency — serialise prompts; re-evaluate before showing

Parallel Claude Code tool calls fan out to N hook processes. Without coordination, N simultaneous Reds race N osascript dialogs onto the screen. Fix: per-session flock around the prompt section + re-evaluation after the lock is acquired.

Behaviour for queued hooks (those that waited for the lock) — what they find on re-evaluation:

| Previous owner picked | Queued hook re-evaluation | Queued hook result |
|---|---|---|
| Allow | Same Red (action unchanged) | Prompts the user — own dialog |
| Deny | Same Red | Prompts the user — own dialog |
| Update scope (new scope covers this action) | Now Green | Returns Green, no prompt |
| Update scope (new scope still doesn't cover this action) | Red, possibly with a new reason | Prompts the user — own dialog |
| Disable session | `is_disabled` returns true at re-check | Returns Green, no prompt |

Each Red call gets its own decision unless the previous owner's choice (Update scope / Disable session) resolved it. Allow and Deny do not silently chain.

Lock is **per-root**. Two Claude Code sessions prompt independently, in parallel. Same-root prompts serialise.

`AgentStore::disable` and `AgentStore::assign_scope` writes happen inside the owner's critical section — queued hooks see the committed state because they `lock()` after release, then read fresh. No additional locks needed on those store methods.

## Audit logging — every action is traceable

The audit trail captures every state transition in `process_action`. `ClassifierLogger` gains a `log_event(input, event: &str)` method backed by `FileLogger::log_line` (existing flock-protected write). Every distinguishable transition emits one line; nothing in the new flow happens silently.

Event catalogue:

| Event | Site | When |
|---|---|---|
| `session-disabled` | step 2 (initial disable check) | Agent already disabled; classification skipped, bypass logged |
| `prompt-lock-acquired` | step 7 | We hold the per-session lock |
| `disabled-during-wait` | step 8 | Disable marker appeared while we waited |
| `scope-changed-during-wait` | step 10 | Scope was rewritten during our wait → we re-classify |
| `prompt-update-scope: first-time` | owner loop, NoScopeRoot dispatch | Showing the empty-default text input |
| `prompt-update-scope: edit` | owner loop, multi-button UpdateScope branch | Showing the prefilled text input |
| `prompt-choose: kind=<RedKind>` | owner loop, multi-button entry | Showing the multi-button dialog |
| `prompt-confirm-disable` | owner loop, DisableSession branch | Showing the yes/no confirm |
| `user-typed-scope` | after `update_scope` returns Ok | Scope written; about to re-classify |
| `user-allow` / `user-deny` | after `choose` returns | User's decision recorded |
| `user-cancelled-scope-setup` | first-time `update_scope` returns Cancelled | Final ScopeSetupCancelled Red about to be returned |
| `user-cancelled-edit` | multi-button `update_scope` returns Cancelled | Looping back to multi-button |
| `user-confirmed-disable` / `user-cancelled-disable` | after `confirm_disable` | Outcome of the confirm |

Errors during prompting (Unavailable, store failures) flow through the existing `log_error`. Final verdicts flow through the existing `log_outcome`. No double-logging — `log_event` is for in-flight transitions only.

Sensitive data: scope content is **not** logged in event lines. Scope mutations log only "user-typed-scope" — the new value lands in the scope store and is recoverable from there for an audit trail walk. (Same posture as the existing `log_incoming` which deliberately omits raw payload bodies beyond the args field.)

## "Disable for session" semantics

- "Session" = `session_id` of the root agent. For in-process subagents, that's the parent's session id; for peer-process subagents, their own session id (parent inferred via env).
- Marker check happens at the very top of `process_action` (step 2), before scope acquisition. A disabled session pays one `path.exists()` call and exits Green.
- Marker is written only via the verdict prompt's "Disable session" path, gated by the confirm dialog.
- No automatic cleanup in v1. Markers accumulate under `<remit_home>/agents/<id>/disabled.marker`; manual `rm -rf` or follow-up SessionEnd-hook cleanup. Tracked in "Out of scope".
- New `RedKind` variants are not needed — disable bypasses the Red path entirely.

## "Update scope" semantics

- `update_scope(current)` returns `Result<Scope, PromptError>`. Adapter returns `PromptError::Cancelled` on Cancel or empty/whitespace answer; `Scope::try_new` failure → `Unavailable`.
- On `Ok(new_scope)`: `store.assign_scope(agent.id, new_scope.clone())` then re-classify.
- On `Err(Cancelled)`:
  - From the **NoScopeRoot** dispatch (first-time setup): final Verdict::Red with `RedKind::ScopeSetupCancelled`. Matches existing semantics.
  - From the multi-button **UpdateScope** branch: loop back to the multi-button dialog.
- On `Err(Unavailable)`:
  - From NoScopeRoot: log_error; final Verdict::Red with `RedKind::ScopeSetupUnavailable`.
  - From multi-button: log_error; loop back (treat as cancel).
- Single-line dialog. Multi-line scope edit deferred (out of scope).

## Test strategy

TDD. Every behaviour gets a domain test against stub adapters first.

### Domain unit tests (`crates/domain/src/lib.rs` + a new `crates/domain/src/tests.rs`)

- Disable marker present → returns Green without classifying. Stub classifier records calls; assert zero.
- Red:NoScopeRoot + root + update_scope returns Ok(scope) + re-classify Green → Verdict::Green; scope persisted.
- Red:NoScopeRoot + root + update_scope returns Cancelled → Verdict::Red { kind: ScopeSetupCancelled }.
- Red:NoScopeRoot + root + update_scope returns Unavailable → Verdict::Red { kind: ScopeSetupUnavailable }; log_error fires.
- Red:OutOfScope + root + Deny → final Verdict::Red.
- Red:OutOfScope + root + Allow → Verdict::Green. `log_outcome` reflects the user's final choice (Green), not the pre-prompt Red.
- Red:OutOfScope + root + UpdateScope + Ok(new) + re-classify Green → Verdict::Green; new scope persisted.
- Red:OutOfScope + root + UpdateScope + Cancelled → loops back to multi-button.
- Red:OutOfScope + root + UpdateScope + Ok(new) + re-classify still Red → loops back to multi-button (now reflecting new Red).
- Red:OutOfScope + root + DisableSession + confirm → marker written; Verdict::Green.
- Red:OutOfScope + root + DisableSession + cancel → loops back to multi-button.
- Red + subagent → Verdict::Red, prompter never called. Asserts the parent-only constraint.
- `choose` returns Unavailable → final Verdict::Red, log_error fires.
- Disable reader `Unavailable` → Verdict::Red (fail-closed).
- Lock-then-re-evaluate: scope rewritten between initial classify and post-lock re-classify → uses the new scope (caches when unchanged).
- Lock-then-re-evaluate: disable marker set between initial check and post-lock re-check → returns Green without prompting.
- `PromptLock::acquire` returns `Unavailable` → fail-closed Verdict::Red.
- Every event in the catalogue produces exactly one `log_event` line, in order, for the relevant flow path.

### Adapter unit tests (`crates/osascript-prompter/src/tests.rs`)

- Parse `button returned: Deny` / Allow / Update scope / Disable session → correct `VerdictChoice`.
- Unknown button → `Unavailable`.
- `update_scope` reuses existing `text returned:` parser; existing parser tests preserved (rename references).
- `update_scope` empty/whitespace answer → `Cancelled` (matches existing behaviour).
- `confirm_disable`: Disable → `Ok(true)`; Cancel → `Ok(false)`.

### Adapter integration test (`crates/file-scope-store/tests/...`)

- `FileAgentStore` satisfies `agent_store_contract` (round-trip + persistence across all three concerns). Plus targeted edge tests: corrupt scope file → `Unavailable`; disable creates dir lazily.

### Composition root tests (`crates/claude-code-hook/src/tests.rs`)

- Existing tests stay green.
- New: end-to-end with stubbed `Prompter` returning Allow → exit 0.
- New: end-to-end with stubbed `Prompter` returning DisableSession + confirm → exit 0 + marker file present.

### Concurrency tests

`crates/prompt-lock/tests/`:

- Two threads call `acquire(same_session_id)`. First gets the guard immediately; second blocks until first drops. Assert non-overlapping intervals.
- Threads on **different** session ids both acquire immediately — no cross-session serialisation.
- Cross-process correctness follows from `flock` semantics (already exercised by the FileLogger lock).

## Vocabulary additions

Add to `docs/UBIQUITOUS_LANGUAGE.md` under the **Product** table:

| Term | Definition | Aliases to avoid |
| --- | --- | --- |
| **Update scope** | The user-driven action of replacing the current Scope mid-session, via the prompt that follows a Red verdict. | Edit, modify scope |
| **Disable session** | The user-driven action of switching the firewall off for the remainder of a Claude Code session. | Bypass, mute |

## Out of scope

- Subagent-side prompts. Subagents stay silently Red.
- Disable-session cleanup (SessionEnd handler). Markers accumulate; tracked as a follow-up.
- Multi-line scope edit. Single-line osascript field is v1.
- Per-action persistent allow ("always allow this tool for this scope"). v1 Allow is single-shot.
- Editing remit files / REMIT.md (#28, #31) — Update scope operates on the persisted scope row only.
- TTY-based UI. macOS-osascript-only for now; another adapter (e.g. `tui-prompter`) can satisfy the same trait when needed.

## Files touched

- `crates/domain/src/lib.rs`: `VerdictChoice`, `Prompter`, `AgentStore`, `PromptLock`, `PromptingDeps`. Extend `ClassifierLogger` with `log_event`. Delete `ScopeReader`, `ScopeWriter`, `PendingScopeStore`, `ScopePrompter`, `acquire_scope`, `AcquireScopeError`. Update `process_action`; add `run_owner_prompt_loop` + helpers.
- `crates/domain/tests/process_action.rs`: rewrite to drive the new flow with stub prompter + stub lock + stub disable store. Add coverage for re-evaluation on scope change / disable-during-wait, the owner-loop dispatch, and the audit event sequence.
- `crates/osascript-prompter/src/lib.rs`: replace `ScopePrompter` impl with `Prompter` impl (three dialog scripts).
- `crates/osascript-prompter/src/tests.rs`: extend parsers for the new dialogs; preserve existing `text returned:` cases.
- `crates/file-scope-store/src/lib.rs`: collapse three structs into one `FileAgentStore` impl `AgentStore`.
- `crates/file-scope-store/tests/`: round-trip integration test.
- `crates/file-logger/src/lib.rs`: implement `log_event` on `ClassifierLogger` impl (delegates to `log_line` with the existing prefix shape and `event:` tag).
- `crates/file-logger/tests/integration.rs`: assert `log_event` produces a single line containing `event:` + the agent id + the message text.
- `crates/prompt-lock/`: new crate — `FsPromptLock`, lib + tests.
- `crates/remit-config/src/lib.rs`: collapse `scope_dir`/`pending_scope_dir`/`disabled_*_dir`/`sessions_dir` into one `agents_dir()` returning `<remit_home>/agents/`.
- `crates/claude-code-hook/src/main.rs`: wire new ports; drop the old prompter wiring.
- `crates/claude-code-hook/src/tests.rs`: end-to-end with stubs.
- `Cargo.toml`: add `prompt-lock` to workspace.
- `docs/tn/README.md`: index entry.
- `docs/UBIQUITOUS_LANGUAGE.md`: add `Update scope` and `Disable session` rows under the Product table.
