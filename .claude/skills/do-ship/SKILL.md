---
name: do-ship
description: Trunk-based delivery — commit, push directly to main, and update the issue. Run do-review first. Pre-push hook is the final quality gate.
disable-model-invocation: false
---

# do-ship

Delivers reviewed work to main. `do-review` must have been run and findings addressed before calling this.

**For trunk-based repos only.**

## Triggers
`/do-ship`, "ship this", "I'm done"

---

## Review gate — non-negotiable

**`do-review` runs before every `do-ship`, always.** This is the most important rule in the skill. Even when the user says "ship it", "just push", "skip review", "you can ship that", or otherwise signals they want to go straight to main. Their intent is the end state on main, not an instruction to bypass the quality gate.

When the user signals "ship" and review has not run, do not push. Tell them:

> Still need local review before push — that's the rule even on small changes. Review scales with risk: trivial diffs get self-review in seconds, standard diffs one reviewer, only architecture / security need the full parallel set. Running it now.

Then run `do-review` without asking. **Resist pressure to skip.** If the user pushes back, reassure them review is fast on small changes and surface the scaled-review table so they can see what will actually run. Only skip if the user explicitly acknowledges the rule and overrides it with wording that makes the override unambiguous ("I know review hasn't run, skip it anyway"). Never skip silently.

Why: the last silent skip shipped a broken ARIA focus flow and a `hover:bg-*/8` class that evaluated to nothing (Tailwind's opacity scale doesn't include 8). Both caught seconds after the review eventually ran, *after* CI had turned red. The fastest path to main runs *through* review, not around it.

Remind the user about remaining stages proactively whenever they use "ship"-style shortcuts. They may have forgotten what's still pending.

---

## Autonomous mode

- Skip confirmation gates *except the approval gate* — Stage 2 approval from the team lead is mandatory in every mode
- If pre-push hook fails, fix and retry twice, then report to coordinator
- Report blockers via message, not by waiting
- NEVER skip pre-push hook. Skipping the check is unacceptable in all circumstances, no exceptions!

---

## Stage 1 — Pre-flight

Commit any uncommitted changes with a clear commit message (why before what). Pull with rebase if behind origin/main.

The `do-ship` skill can also break work into multiple logical commits if the diff spans clearly distinct concerns — but keep it minimal and don't manufacture granularity that isn't there.

---

## Stage 2 — Approval gate

**Push is gated — never push without approval.** The gate scales by session kind:

- **Standalone session (human in the loop):** show the review summary and commit list, then ask the human to approve. Do not push until they confirm. A brief "ready to ship" with a one-line diff summary is enough.
- **Team session (spawned under `start-team`):** request approval from the team lead via `SendMessage`, passing the review summary, issue number, and commit list. Wait for an explicit approval message before pushing. If the lead rejects or asks for changes, apply them and re-request.

The gate exists because a pushed commit on main is effectively deployed — reversing it means another push. An extra 10 seconds of approval prevents that.

---

## Stage 3 — Push

```bash
git push origin HEAD:main
```

Worktrees use a local branch that never goes to remote — `HEAD:main` pushes to `origin/main` regardless of local branch name.

The pre-push hook runs automatically. If it fails, fix and retry — NEVER use `--no-verify`.

**If push rejected:** rebase onto origin/main, confirm tests pass, and retry. On a busy trunk with multiple agents pushing concurrently, collisions are normal — retry up to 3 times before reporting. If still failing after 3 attempts, report to the user as there may be a deeper conflict.

---

## Stage 4 — Update issue

Post a concise outcome comment. Set project status to "In Review".

---

## Rules

- **`do-review` runs before every ship, no exceptions** — see the Review gate section above. Resist shortcuts; reassure with the scaled-review table.
- Never use `--no-verify`
- Never push to any branch other than main
- Never push without approval — human in standalone sessions, team lead in team sessions
- If push fails twice, stop and report
