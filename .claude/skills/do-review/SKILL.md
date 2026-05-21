---
name: do-review
description: Pre-ship review scaled to the risk of the change. Selects the right reviewers, applies findings inline, and produces an issue comment summary. Run before do-ship.
disable-model-invocation: false
---

# do-review

You are a thorough, experienced code reviewer. Getting it right matters more than getting it done. Findings are applied inline before `do-ship`, not noted for later.

## Triggers

`/do-review`, "review my changes", "check the architecture", "review this"

---

## Stage 1 — Read the diff and classify risk

Read the full diff and any context needed (changed files, related tests, impacted interfaces).

| Risk level       | Criteria                                                                               |
| ---------------- | -------------------------------------------------------------------------------------- |
| **Trivial**      | Docs, comments, config, text, annotations only                                         |
| **Standard**     | Single domain, clear scope, no security/auth/ports touched                             |
| **Architecture** | New abstractions, layer changes, cross-cutting, ports, new patterns or complex feature |
| **Security**     | Auth, crypto, permissions, data handling, wire format, external inputs                 |

A change can be both Architecture and Security — run both reviewers.

---

## Stage 2 — Select and invoke reviewers

### Model selection

| Model                | When                                                                                                                       |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `opus` (Opus 4.6 1M) | Architect — always. Security reviewer for complex auth/crypto. Any review where the stakes are high or the domain is deep. |
| `sonnet` (default)   | Security analyst for standard findings. Code quality and test coverage reviewers.                                          |
| `haiku`              | Documentation scan only — fast, low-stakes.                                                                                |

When in doubt, use `opus`. A missed architectural finding costs more than a stronger model.

### The reviewers

**Architect** (`architect`) — **always `model: opus`**

> Hexagonal architecture compliance, layer boundary violations, naming consistency, separation of concerns, duplication, structural patterns.
> Hard stop if findings require an ADR.

**Security analyst** (`security-analyst`) — `model: opus` for auth/crypto/permissions; `sonnet` for standard checks

> OWASP top 10, input validation, secret handling, auth/crypto correctness, dependency risk.
> Hard stop if findings require an ADR.

**Code quality reviewer** — `sonnet`

> Clarity, readability, unnecessary complexity, dead code.

**Test coverage reviewer** (`test-coverage-agent`) — `sonnet`

> Coverage gaps, missing edge cases, test quality. The most valuable finding is an edge case that actually breaks the implementation — a test that legitimately fails and proves the code needs to change. Hunt for those actively.

**Domain language reviewer** (`ubiquitous-language`) — `sonnet`

> Naming consistency with the domain model.

### Selection

| Risk level   | Reviewers                                                  |
| ------------ | ---------------------------------------------------------- |
| Trivial      | Self-review only — no agents                               |
| Standard     | Architect (`opus`)                                         |
| Architecture | Architect (`opus`)                                         |
| Security     | Architect (`opus`) + Security analyst (`opus` or `sonnet`) |

Add `test-coverage-agent` or `ubiquitous-language` when the diff warrants it. Spawn in parallel where possible.

Lean on being more rigorous than fast. Quality is valued over speed in all cases.

---

## Stage 3 — Apply findings

**Enumerate first.** Before writing any code, list every finding from every reviewer with a disposition: **Implement / Disagree / Defer**. The summary must account for all of them.

- **Implement** — default. No justification needed.
- **Disagree** — finding is wrong, inapplicable, or based on a misread. State reasoning in the summary.
- **Defer** — only when applying the finding would expand the diff *significantly outside* the files already touched, or require architectural changes not in the original plan. Renames, extractions, restructuring, or non-trivial rewrites *within* the touched files do not qualify — those are in scope. Create a follow-up issue and note the gap in the summary.

Severity is not a disposition. A "Low" finding inside touched files is Implement. "It's only a medium" and "we can do it later" are not valid reasons to defer.

**Full rework escalation** — if the architect reviewer concludes that the implementation is so fundamentally misaligned (wrong layer, wrong pattern, wrong approach) that fixing individual findings would produce worse code than starting fresh:

Do not enumerate findings. Instead:

1. State clearly: "Architect review: full rework required — [one-paragraph reason]."
2. Post this as an issue comment.
3. Set issue status back to **Ready**.
4. **Autonomous mode:** report to the team lead via `SendMessage`: "Review failed on #N — full rework required. [reason]. Shutting down." Then shut down — do not attempt fixes.
5. **Manual mode:** tell the developer: "The architect reviewer recommends a fresh approach to this issue. The current implementation [reason]. The issue has been reset to Ready — please reconsider the design before picking it up again."

Do not merge partial fixes with a fundamentally flawed implementation. A clean restart is faster than patching the wrong foundation.

---

**Hard stop — ADR required** if the diff:

- Deviates from hexagonal architecture (business logic outside domain, bypassed ports, wrong layer coupling)
- Touches security model invariants
- Introduces a structural pattern not established in the codebase

**Hard stop — CDR required** if the diff breaks an existing coding convention without a record in `docs/cdr`

Either case, rewrite to comply with existing architecture/conventions or raise with user before proceeding (Status: Needs Human). Do not ship.

---

## Stage 4 — Documentation review

Refer to the repository's main instructions (root `CLAUDE.md` / `AGENT.md` / equivalent) for documentation locations and conventions.

For each doc area the repo declares: does the diff add, remove, or change something it describes? If yes, update it in the same change.

Do not ship with stale documentation.

---

## Stage 5 — Summary

Post to the issue:

```
Risk: [Trivial / Standard / Architecture / Security]
Reviewers: [list]
Findings applied: [list or "none"]
Findings disagreed with: [finding + reason, or "none"]
Follow-up issues created: [list or "none"]
Docs updated: [list or "none"]
```

---

## Rules

- **Every finding gets a disposition: Implement / Disagree / Defer.** Default is Implement. Defer only when applying would significantly expand the diff outside touched files or require unplanned architectural changes — not for severity, not for "later". Disagree requires reasoning in the summary.
- Architect always uses `model: opus`
- ADR/CDR escalation is a hard stop — no exceptions
- Documentation must be updated before shipping — stale docs are a bug
- Summary always posted, even for trivial ("self-reviewed, clean")
