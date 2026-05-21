---
name: do-status
description: Dashboard of all active worktrees — branch, linked issue, GitHub Project status, and PR state. Read-only. Flags stale worktrees and anything needing attention.
---

# do-status

A read-only overview of all active work. Shows every worktree, what ticket it's linked to, where it is in the workflow (from GitHub Projects), and whether anything looks stale or needs action.

## Triggers

`/do-status`, "what's in progress", "show worktrees", "what's open", "what am I working on"

---

## Step 1 — List Worktrees

```bash
git worktree list
```

---

## Step 2 — Resolve Issue, Project Status, and PR Per Worktree

For each linked (non-main) worktree:

```bash
BRANCH="<branch>"
ISSUE_NUM=$(echo "$BRANCH" | grep -oE '^[0-9]+')

# Issue state + project status
gh issue view "$ISSUE_NUM" --json number,title,state,labels,projectItems \
  --jq '{number, title, state, labels: [.labels[].name], project: .projectItems[0].project.title, status: .projectItems[0].status.name}' 2>/dev/null

# If status is Blocked or Needs Human, get the "Blocked reason" field
gh issue view "$ISSUE_NUM" --json projectItems \
  --jq '.projectItems[0].fieldValues' 2>/dev/null

# PR state
gh pr list --head "$BRANCH" --state all \
  --json number,title,state,url --jq '.[0]' 2>/dev/null
```

---

## Step 3 — Render Dashboard

```
Active worktrees
--------------------------------------------------------------------------------
 Worktree                         Branch              Issue        Status       PR
--------------------------------------------------------------------------------
 ~/dev/myapp                      main                —            —            —
 ~/dev/myapp-wt/42-auth           42-auth-refactor    #42 open     In Review    PR #87 open
 ~/dev/myapp-wt/55-model          55-data-model       #55 open     In Progress  — (not raised)
 ~/dev/myapp-wt/61-login-fix      61-fix-login        #61 closed   Done         PR #91 merged
 ~/dev/myapp-wt/70-cache          70-add-cache        #70 open     Blocked      — (not raised)
--------------------------------------------------------------------------------
```

**Status** column from GitHub Project status:

- Backlog, Next, Ready, In Progress, Blocked, Needs Human, In Review, Done

**Annotations:**

- If status is "Blocked" or "Needs Human", show the "Blocked reason" field value on a second line
- If `needs-human` label is present, append " [HUMAN]"

**PR** column:

- open / merged / closed-no-merge / — not raised

---

## Step 4 — Callouts

After the table, flag anything notable:

- **Stale worktree:** PR merged/closed but worktree still exists — "`.../61-login-fix` looks stale — PR merged. Run `do-close`."
- **Skipped review:** PR exists but status is not "In Review" — "Issue #N was shipped without going through review."
- **Branch behind main:** fetch and check — "Branch `42-auth-refactor` is N commits behind main. Run `do-sync`."
- **No issue inferred:** branch name doesn't match convention — "Branch `old-experiment` has no associated issue."
- **Blocked items:** any issue with status "Blocked" or "Needs Human" — surface the blocked reason.

---

## Rules

- Read-only — never modify issues, PRs, or branches
- Never guess issue numbers if branch naming doesn't match the convention
