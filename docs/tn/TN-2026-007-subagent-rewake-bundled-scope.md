---
number: 2026-007
title: Subagent re-wake bundled scope
kind: plan
status: active
issue: "#25"
tags: [scope, hook, bundled-scope, send-message]
---

# TN-2026-007: Subagent re-wake bundled scope

> Re-wakes via `SendMessage` arrive with a fresh `agent_id` and no pending row. Children fail-closed on first tool call. Fix: treat `SendMessage.content` as the bundled scope for the round.

## Failure mode

- Parent calls `SendMessage` to a named (or in-process) subagent.
- A `SubagentStart` fires with a brand-new `agent_id`.
- `claim_and_assign` calls `PendingScopeStore::take(parent)` → miss (parent never staged for this round).
- Child's first `PreToolUse` → `scope_for(child_id)` returns `None` → `Classifier` emits `Verdict::Red { kind: SubagentMissingScope }`.
- Observed 9+ times in a 13-min window across multiple `SendMessage` rounds.

## Why it happens

- Pending-scope state machine assumes **first contact**: each SubagentStart pairs with one prior parent spawn that staged a row. `Agent` spawns satisfy this; `SendMessage` re-wakes don't.
- `BUNDLED_SCOPE_FIELDS` (in `claude-code-hook`) recognises only `("Agent", "prompt")`. `SendMessage` is invisible to the staging step.
- Each re-wake gets a fresh `agent_id`, so the child has no scope row even though the same logical subagent ran scoped on a previous round.

## Fix

- Recognise `SendMessage` as a scope-bearing parent action.
- Extend `BUNDLED_SCOPE_FIELDS` with `("SendMessage", "content")` — the round's directive to the named subagent.
- The existing `stage_pending` → `claim_and_assign` flow handles the rest. Single-slot pending still applies.

### Why this fix, not the others

- **"Look up an existing active scope"** (parent walk / id-aliasing across re-wakes): rejected by [TN-2026-003](TN-2026-003-session-scope-acquisition.md). Each Agent has its **own** scope; no inheritance.
- **Persist bundled scope across re-wakes** (non-consuming `take`): would silently re-bind the previous round's scope to a new round's child. The parent's directive may have changed; pinning yesterday's scope hides drift.
- **Key pending by `(parent, recipient_name)`**: tracked with #20 as the FIFO migration. Out of scope here — needs upstream `parent_tool_use_id` or recipient on the start payload to land cleanly.

### Layer placement

- `BUNDLED_SCOPE_FIELDS` is wire knowledge — which Claude-Code tool field carries scope text. Lives in `claude-code-hook` (the adapter), not domain. Architecture rule: wire types stay in adapter crates.
- The fix is one entry in that table plus a unit test. No domain change required. The domain state machine is correct; what was missing was an adapter telling it about a second scope-bearing tool.

## Test strategy

- **Failing reproduction first.** Unit test in `claude-code-hook`: `bundled_scope_from("SendMessage", {"content": "do X", ...})` returns `Some("do X")`. Currently returns `None`.
- Snapshot test on `BUNDLED_SCOPE_FIELDS` updated to include the new pair.
- Existing `("Agent", "prompt")` cases stay green — additive change.
- Empty / whitespace / missing `content` rejected by the existing trim filter; covered by the parametrised allowlist tests.

## Directional assumption

`Agent.prompt` is unambiguously parent→child intent. `SendMessage.content` isn't — it can flow in any direction. Three cases observed in `~/.remit/remit.log` from real team runs:

- **Parent → child re-wake.** `SubagentStart` fires with `parent_id = sender`, fresh `agent_id`. The fix stages and the start hook claims. ✓ Confirmed at log lines 3105–3108 (coordinator → `dev-4`) and 3137 + claim (`dev-6-2`).
- **Child → parent report-back.** No `SubagentStart` fires — the parent is already running. The new fix stages a row under the child's id that nothing claims. Cleared by single-slot overwrite if the child later does an `Agent` spawn; otherwise orphaned. Pollutes the pending store but does not bind a wrong scope. Confirmed at lines 142, 2201, 2245, 2699, 2753, 3032, 3125, 3342, 3463, 3575 — every child→coordinator round in the trace produced no claim attempt.
- **Peer ↔ peer.** Not observed. Every team round in the trace is flat: workers message `coordinator`, never each other. **If** a peer-to-peer workflow appeared **and** the recipient's `SubagentStart` carried `parent_id = sender` (which the runtime does for case 1, so it's plausible), the recipient would inherit the sender's message text as scope. Real risk, currently theoretical.

Revisit if peer-to-peer SendMessage workflows appear. Mitigations on the table: filter by recipient identity (only stage when the recipient is a known subagent of the caller), or move to `(parent, recipient_name)` pending keying as part of #20.

## Out of scope

- FIFO pending queue / `(parent, recipient_name)` keying — #20.
- Stale pending file cleanup — #20.
- Other `SendMessage`-shaped tools (none today).
