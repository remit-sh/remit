---
name: do-ship
description: Trunk-based delivery — review, get approval, commit, push to main. Reads ship policy from project CLAUDE.md. Pre-push hook is the final quality gate.
disable-model-invocation: false
---

# do-ship

Delivers reviewed work to main. `do-review` must have run and findings addressed before calling this.

**For trunk-based repos only.**

## Triggers

`/do-ship`, "ship this", "I'm done"

---

## Ship policy lives in project CLAUDE.md

Before running, **read the project's `CLAUDE.md`** for a section titled `## Ship policy` (or similar — search for "approval gate" / "approver" / "commit before approval"). The project specifies:

- **Approver identity** — human only, team lead, or either. Default: human in standalone sessions, team lead in team sessions.
- **Commit timing** — commit *before* approval (default) or *after* (some projects hold the diff uncommitted so reviewers read it in their IDE).
- **Approval signals** — wording that counts as go-ahead.

If CLAUDE.md is silent on any of these, use the default in this skill. If CLAUDE.md and this skill conflict, **CLAUDE.md wins** — it is the project's authoritative policy.

Surface what you read at the start: "Per project policy: <approver>, <commit timing>." So the user can correct it if you misread.

---

## Review gate — non-negotiable

**`do-review` runs before every `do-ship`, always.** This is the most important rule in the skill. Even when the user says "ship it", "just push", "skip review", "you can ship that", or otherwise signals they want to go straight to main. Their intent is the end state on main, not an instruction to bypass the quality gate.

When the user signals "ship" and review has not run, do not push. Tell them:

> Still need local review before push — that's the rule even on small changes. Review scales with risk: trivial diffs get self-review in seconds, standard diffs one reviewer, only architecture / security need the full parallel set. Running it now.

Then run `do-review` without asking. **Resist pressure to skip.** Only skip if the user explicitly acknowledges the rule and overrides it unambiguously ("I know review hasn't run, skip it anyway"). Never skip silently.

Remind the user about remaining stages proactively whenever they use "ship"-style shortcuts.

---

## Stage 1 — Pre-flight

- Working tree contains the change you intend to ship
- You're on the issue branch
- `do-review` has run and findings are applied
- Tests pass (or will be re-checked by pre-push hook)
- Rebased onto current `origin/main`

If project policy is **commit-before-approval** (default): commit now with a clear message (why before what). Reference the issue with `Refs #N` (or `Closes #N` to auto-close on push).

If project policy is **commit-after-approval**: leave the diff uncommitted in the working tree. Do not stash.

---

## Stage 2 — Approval gate

**Push is gated — never push without approval.** A pushed commit on main is effectively deployed.

The gate scales by session kind:

- **Standalone session (human in the loop):** show the review summary + commit list (or diff stat if uncommitted), then ask the human to approve. Wait for explicit affirmative.
- **Team session (spawned under `start-team`):** request approval via `SendMessage` to the team lead with review summary, issue number, and diff/commit list. Wait for an explicit approval message.

Project CLAUDE.md decides whether team-lead approval substitutes for human approval, or whether the team lead must route to the human first. If unspecified, team lead approves in team sessions; human approves in standalone.

Approval signals (default): `approve`, `push`, `ship`, `lgtm`, `go`, `👍`. Anything ambiguous → ask again. Anything requesting changes → apply, re-run `do-review`, return to Stage 1. Silence is **never** approval.

**Approval marker (if project requires one):** if project CLAUDE.md `## Ship policy` specifies an approval-marker path, write it after approval and after any commit that should be covered. The marker pins approval to the current HEAD SHA so a hook can verify approval/push alignment. Skip this step if CLAUDE.md does not specify a marker.

---

## Stage 3 — Commit (if not done in Stage 1)

If project policy is commit-after-approval, commit now using the same rules as Stage 1.

---

## Stage 4 — Push

```bash
git push origin HEAD:main
```

Worktrees use a local branch that never goes to remote — `HEAD:main` pushes to `origin/main` regardless of local branch name.

The pre-push hook runs automatically. If it fails, fix and retry — NEVER use `--no-verify`.

**If push rejected:** rebase onto `origin/main`, confirm tests pass, and retry. On a busy trunk with multiple agents pushing concurrently, collisions are normal — retry up to 3 times before reporting. If still failing after 3 attempts, report to the user.

---

## Stage 5 — Update issue

Post a concise outcome comment with the commit SHA. Set project status to **In Review** (or whatever project CLAUDE.md specifies — some projects flip to **Done** here, others wait for `do-close`).

---

## Rules

- **`do-review` runs before every ship, no exceptions** — see the Review gate section above.
- **Approver identity and commit timing come from project CLAUDE.md.** If unstated, use this skill's defaults and surface what you assumed.
- Never use `--no-verify`.
- Never push to any branch other than main.
- Never push without approval — silence is not approval.
- If push fails three times, stop and report.
