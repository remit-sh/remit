# Architect review — TN-2026-013

> Two-pass classification + session history + workflow rules. Proposal under review.

## 1. Hex compliance — port shapes and adapter boundaries

**Ports.** `ActionSummariser` and `SessionHistory` are clean domain-side concepts; both can be expressed as traits with domain-only signatures (`Action`, `AgentId`, a newtype `ActionSummary`, a domain `SessionHistorySnapshot`). No filesystem types or wire shapes need leak in. **Fine in principle.**

**Adapter → adapter rule.** The proposal says `file-logger-history` "doesn't depend on the `file-logger` crate" — but it reads the file that `file-logger` writes. That's a *runtime* coupling (shared on-disk schema) even if the *compile-time* graph is clean. ADR-0001 forbids the import; it doesn't say much about a de-facto shared file format. This is the same kind of cross-adapter contract that ADR-0001 was designed to prevent eroding silently. **Flag.**

The log file becomes a load-bearing inter-adapter contract with no domain-owned schema. The moment `file-logger`'s NDJSON shape changes, `file-logger-history` breaks. Two clean ways out:

- **A.** Pull a small `ClassificationLogEntry` schema into domain (just the fields read back, not the rendering). Both adapters depend on it.
- **B.** Merge the two into one adapter `file-logger` that both writes and reads — split into `ClassifierLogger` (write) and `SessionHistory` (read), both implemented by the same crate.

**Recommend B.** Models the world honestly (one log file → one adapter) and avoids the second-adapter-by-stealth.

**Verdict: ⚠️ flag.** Resolve before promotion.

## 2. Domain shape — widening `LlmClassifier::classify`

Proposal widens to `(ActionSummary, SessionHistorySnapshot, StandingGuidance)`. Three observations:

- `StandingGuidance` already lives on `ClassificationContext` (TN-008 landed). The "make it explicit" line is a no-op. **Drop that bullet.**
- `ActionSummary` is a domain newtype — fine. **Clean.**
- `SessionHistorySnapshot` entries `{timestamp, action_kind, summary, verdict}` are all domain types. **Clean.**

Under-specified: how pass 1 and the snapshot are composed. Two options:

- Widen `ClassificationContext` to carry `summary` and `history`. Composition root produces both before calling `Classifier::classify`. **Recommended** — symmetric with how `StandingGuidance` lands today.
- Or hand `ActionSummariser` + `SessionHistory` to `Classifier` itself. Worse: drags `Classifier` away from "pure decide-and-return". `process_action` already owns orchestration.

**Verdict: ✅ fine with revisions.**

## 3. Vocab fit — "Procedure" as a new Standing guidance slot

**Right instinct, wrong term.**

- **Slot vs concept conflation.** "New top-level concept (Procedure)" vs "new slot under Standing guidance" are different decisions.
- **"Procedure" is overloaded.** "Workflow rule" used informally in the proposal is actually clearer.

**Suggested split:**

- Third slot of **Standing guidance** alongside **Project guidance** and **Agent template**. Call it **Workflow guidance** (parallel naming).
- Don't introduce a sibling to **Scope**. Scope stays relevance; Workflow guidance is just another trusted-system-prompt input.
- No `Procedure` domain noun needed if there's no domain type for it.

Collapses the vocab change to a single new entry under Classification, parallel to the existing two.

**Verdict: ⚠️ flag.** Drop "Procedure"; rename to "Workflow guidance".

## 4. Coupling — three changes, one TN

The chain is one-directional: workflow rules depend on history depends on two-pass. The reverse isn't true.

- **Two-pass is independently valuable** — closes TN-005 #34/#35 narration-strip workarounds even without history or rules.
- **History is independently valuable** — lineage hardening (TN-005 deferred) + audit benefit immediately.
- **Workflow rules require both.**

**Recommend splitting:**

- **TN A (now)**: Pass-1 summary + classifier widens to take `ActionSummary`.
- **TN B (next)**: `SessionHistory` port + `file-logger` extended to read its own log. Snapshot fed into `ClassificationContext`. No new rules.
- **TN C (after)**: Workflow guidance slot in Standing guidance.

Staggers the latency hit (only TN A doubles LLM calls; TN B and C add none), gives each TN an independent measurement spike.

**Verdict: ❌ block as a single TN.** Split into three.

## 5. Fail-closed ladder — history read soft-fail

ADR-0002 reads bluntly: *"On any error or uncertainty, the verdict is Red."*

The proposal's rationale ("a rule requiring evidence will Red against an empty snapshot anyway") elides the symmetric case: "X must never have happened" — empty snapshot means no X, **Green**. False Green.

**Two options:**

- Match the rest of the ladder: history read fail → `Red(HistoryUnavailable)`. Loudest, consistent. Default.
- Suppress workflow rules entirely on history failure but still run pass 2 on a non-workflow path. Workable but more moving parts.

**Verdict: ❌ block.** Fail-closed on history-read errors.

## 6. Composition root churn

Wiring two more adapters is mechanical and precedented. ADR-0003 anticipates it. Not a problem on its own.

What *would* be a problem: independently-configured logger and history-reader create an "incoherent config" surface. Reinforces §1's recommendation B: collapse the two roles into one adapter.

**Verdict: ✅ fine if §1.B taken.** ⚠️ flag if two separate adapters land.

## 7. Compliance with active ADRs/CDRs

- **ADR-0001 (hexagonal).** Two adapters sharing a file: see §1. **Compliance unclear.**
- **ADR-0002 (fail-closed).** History soft-fail contradicts. See §5. **Non-compliant as drafted.**
- **ADR-0003 (single composition root).** Compliant.
- **ADR-0004 (newtype boundary validation).** `ActionSummary` should be a validated newtype; proposal doesn't say where validation lives. **Flag — call out explicitly.**
- **CDR-0001 (vocab).** See §3 — recommend slot rename rather than a new noun.
- **CDR-0004 (doc system).** Compliant procedurally.
- **CDR-0005 (current-state-only).** Watch for log-schema migration framing.

## Verdict

**reject-and-rework.**

Required revisions before re-submission:

1. **Split the TN into three** (pass-1, history, workflow guidance) — §4.
2. **Fail-closed on history-read errors** — §5. Add `RedKind::HistoryUnavailable`.
3. **Resolve the two-adapters-sharing-a-file question** — §1. Collapse into a single `file-logger` adapter implementing both `ClassifierLogger` (write) and `SessionHistory` (read).
4. **Drop "Procedure" as a noun**; rename to "Workflow guidance" — §3.
5. **Specify `ActionSummary` validation** per ADR-0004 — §7.
6. **Drop the "make StandingGuidance explicit" claim** — already done in TN-008 — §2.
7. **Run the measurement spike before TN A is promoted to plan** — keep it.
