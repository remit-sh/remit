---
name: do-kick-off
description: Sets up a piece of work — confirms the issue, creates a correctly-named feature worktree, and sets status to In Progress. Then hands off to do-plan.
disable-model-invocation: true
---

# do-kick-off

Sets up the work correctly. Run this before anything else.

**If asked to make any file change before kicking off**, refuse:
> "Before I write anything, we need to kick off. Do you have an issue for this, or shall I create one?"

---

## Autonomous Mode

- Issue number provided by team lead
- Skip confirmation gates — proceed with defaults
- Auto-create worktree without asking
- **Hard stop if issue is not assigned to current user** — report to team lead, do not proceed
- Report blockers via message, never wait

---

## Stage 1 — Issue

Search before asking. Confirm the matched issue explicitly — never assume a search match is correct.

If not found, offer to create one. Every issue must have a status, epic, and assignee.

Set project status to **In Progress**.

---

## Stage 2 — Worktree

The main worktree is read-only. All writes happen in a feature worktree.

Worktrees live in a sibling directory named `<project>-worktrees/`. Derive the path from `git worktree list` — never hardcode it.

Both the worktree directory and local branch must be named `<issue-number>-<kebab-slug-of-title>` — e.g. `201-enforce-worktree-development`. The name is the audit trail. Warn and offer to recreate if they don't match the confirmed issue.

If already in a correctly-named worktree for this issue — proceed without recreating.

Output the full worktree path so the user can open a new session there.

---

## Stage 3 — Hand off

Kick-off is complete. Run `do-plan` next.

---

## Rules
- Never write code — kick-off only sets up the context
- Never work in a misnamed worktree
- Never proceed without a confirmed issue assigned to the current user
