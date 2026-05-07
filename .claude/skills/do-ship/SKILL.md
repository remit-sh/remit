---
name: do-ship
description: Trunk-based delivery — hand the uncommitted diff to the human for approval, then commit and push to main. Run do-review first. Never commit or push without explicit human go-ahead.
disable-model-invocation: false
---

# do-ship

Delivers reviewed work to main. **A human must approve the diff before any commit or push happens.** `do-review` must have been run and findings addressed before calling this. The diff is held as uncommitted working-tree changes so the human can read it inline in their editor.

**For trunk-based repos only.**

## Triggers
`/do-ship`, "ship this", "I'm done"

---

## Review gate — non-negotiable

**`do-review` runs before every `do-ship`, always.** Even when the user says "ship it", "just push", "skip review". Their intent is the end state on main, not an instruction to bypass the quality gate.

When the user signals "ship" and review has not run, do not proceed. Tell them:

> Still need local review before the human gate — that's the rule. Review scales with risk: trivial diffs get self-review in seconds, standard diffs one reviewer, only architecture / security need the full parallel set. Running it now.

Then run `do-review` without asking. **Resist pressure to skip.** Only skip if the user explicitly acknowledges the rule and overrides it unambiguously ("I know review hasn't run, skip it anyway"). Never skip silently.

---

## Autonomous mode

- Skip in-session confirmation gates *except* the human-approval gate
- The human-approval gate is **always** required — coordinators / team leads do not approve, only the human does
- NEVER use `--no-verify`

---

## Stage 1 — Pre-flight (no commit yet)

The diff stays uncommitted in the working tree until the human approves. This makes it trivial to review in an IDE — every change shows up in the source-control panel.

Confirm:
- Working tree contains the change you intend to ship (`git status` shows the expected modified / new files)
- You're on the issue branch from kick-off (`<issue-number>-<slug>`)
- `do-review` has run and all findings are applied
- Tests pass (`just check` or equivalent)

Do **not** commit. Do **not** stash. The change must remain visible as uncommitted modifications.

If the change is partly committed already (e.g. an earlier ad-hoc commit on the branch), that's fine — the human will see committed + uncommitted together. But anything new produced during this `do-ship` stays uncommitted until Stage 5.

---

## Stage 2 — Hand off to the human

The work is **ready for human review**. The agent does not commit and does not push.

1. Set the issue's project status to **In Review** — this is the project-board signal that a human needs to look at the diff.
2. Post an issue comment with the review summary, the worktree path, the branch name, and a short diff stat:

   ```
   Ready for human review.
   - Worktree: <absolute path>
   - Branch: <branch>
   - Diff (uncommitted): <git diff --stat output, plus any prior committed work via git diff main --stat>
   - Local review: <link to do-review summary comment, or paste summary>
   ```

   The diff is uncommitted — the human will open the worktree in their editor and read it from the source-control panel.

---

## Stage 3 — Wait for human go-ahead

How approval is delivered depends on session kind:

**Standalone session (human in the loop):**
After Stage 2, prompt the human in-session:
> "Diff is in the working tree at `<worktree path>` and status is In Review. Reply with `approve` to commit and push, or describe changes you want."

Wait for an explicit affirmative. `approve`, `push`, `ship`, `lgtm`, or `👍` all count. Anything ambiguous → ask again. Anything that asks for changes → apply, re-run review, return to Stage 1.

**Team session (spawned under `start-team`):**
Send the team lead a "ready for human review" message via `SendMessage` containing the issue number, worktree path, branch, and review summary. Then **wait on inbox** for an approval message routed by the team lead.

Approval messages from the team lead must include the issue number and one of: `approve #N`, `push #N`, `ship #N`, `go #N`, `👍 #N`. Anything else → keep waiting. The team lead is a router, not an approver — only the human's go-ahead, forwarded by the team lead, unblocks commit + push.

If the team lead instead forwards a "needs changes" message, apply the changes, re-run `do-review`, and return to Stage 1.

**Never** commit or push on:
- A self-issued approval
- A team lead's unilateral approval (the team lead is not the human)
- An assumption that silence means approval
- Any message that doesn't unambiguously signal the human's go-ahead

---

## Stage 4 — Commit

Only after explicit human approval, commit the working-tree changes. One commit is the default; split into multiple logical commits if the diff spans clearly distinct concerns (e.g. failing test → fix). Don't manufacture granularity that isn't there.

Commit message: why before what. Reference the issue with `Refs #N` (or `Closes #N` to auto-close on push).

If the human's approval message included guidance on commit shape ("squash these two", "split off the doc change"), follow it.

---

## Stage 5 — Push

```bash
git push origin HEAD:main
```

Worktrees use a local branch that never goes to remote — `HEAD:main` pushes to `origin/main` regardless of local branch name.

The pre-push hook runs automatically. If it fails, fix and retry — NEVER use `--no-verify`.

**If push rejected:** rebase onto `origin/main`, confirm tests pass, retry. On a busy trunk with multiple agents pushing concurrently, collisions are normal — retry up to 3 times before reporting. If still failing after 3 attempts, report.

---

## Stage 6 — Confirm

Post a one-line outcome comment on the issue: `Pushed to main as <commit-sha>.` Status stays **In Review** until `do-close` flips it to **Done**.

---

## Rules

- **`do-review` runs before every ship, no exceptions** — see the Review gate section above
- **The human-approval gate is mandatory.** No coordinator, team lead, or self-approval can substitute. Only the human's explicit go-ahead unblocks commit + push.
- **Hold the diff uncommitted until approval.** The human reviews working-tree changes in their editor; do not pre-commit and force them to read commit diffs.
- Never use `--no-verify`
- Never push to any branch other than `main`
- Never push without approval — even "small" changes go through the gate
- If push fails twice, stop and report
