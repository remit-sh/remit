---
name: start-team
description: Persistent team coordinator — creates a named Claude Code agent team, scans GitHub Projects for "Ready" issues assigned to the current user, spawns named agents that each run /do-work for one issue, monitors for conflicts, retries on push failure.
disable-model-invocation: false
---

# start-team

Team coordinator for trunk-based development. Creates a **named Claude Code agent team** (not anonymous subagents), picks up Ready issues assigned to the current user, and monitors progress. Agents push directly to main — no branch sequencing, no merge queue.

**Claude Code only.** Named agent teams are a Claude Code feature (`TeamCreate` + named `Agent` invocations). Other editors (Cursor, Windsurf, etc.) lack the named team primitives — developers in those environments run `/do-work` manually per issue.

## Triggers

`/start-team`, "start team", "start the team", "boot up the team"

---

## Stage 0 — Project board health check

Run this once per session before scanning the backlog. Catches hygiene problems before agents pick up stale or malformed issues.

### Required GitHub project workflows

Verify these are enabled on the project (Settings → Workflows). If any are missing, flag to the user before proceeding:

| Workflow                  | Direction                      | Why needed                                  |
| ------------------------- | ------------------------------ | ------------------------------------------- |
| **Auto-add to project**   | New issue → board              | Prevents orphan issues                      |
| **Item closed**           | GitHub close → status Done     | Keeps board in sync when closing via CLI    |
| **Item reopened**         | GitHub reopen → status Backlog | Prevents reopened issues sitting at Done    |
| **Item added to project** | Added → status Backlog         | Every new issue starts with a status        |
| **Auto-close issue**      | Status Done → GitHub close     | Optional but useful for UI-driven workflows |
| **Auto-archive items**    | Closed + stale → archived      | Keeps Done column clean                     |

If a workflow is missing: tell the user which one and where to find it (project Settings → Workflows). Do not proceed silently.

### Orphan check

Issues not on the project board cannot be picked up by agents. Check for any:

```bash
gh api graphql --paginate -f query='
query($cursor: String) {
  repository(owner: "OWNER", name: "REPO") {
    issues(first: 100, states: OPEN, after: $cursor) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title
        projectItems(first: 5) { nodes { project { number } } }
      }
    }
  }
}' --jq '.data.repository.issues.nodes[] | select((.projectItems.nodes | map(.project.number) | index(PROJECT_NUM)) | not) | "#\(.number) \(.title)"'
```

For each orphan: add to the project and set status to Backlog (or appropriate status based on context).

### Status/state mismatch check

```bash
# Open issues with Done status — should be closed
# Closed issues with non-Done status — status should be corrected
```

Query the project for:

- `state == OPEN` and `status == Done` → close the GitHub issue
- `state == CLOSED` and `status != Done` → flip project status to Done

These accumulate when issues are closed via CLI and project workflows are not configured (see above).

### Epic gap check

Any open issue without an Epic field set should be flagged to the user. Do not block on this — report a summary and let the user decide. Valid epics are project-specific; discover them by querying the Epic single-select field options.

---

## Stage 1 — Initialise

### Create a named Claude Code team

This is not a subagent spawn — it is a persistent named team using the `TeamCreate` tool:

```
TeamCreate:
  team_name: "dev-team"
  description: "Developer team for <repo name>"
```

Check for an existing team first:

- If `~/.claude/teams/dev-team/config.json` exists → reuse it, log "Resuming existing team"
- If not → create with `TeamCreate`

The team provides named agent coordination, shared task lists, and direct message passing. Each agent is spawned with an explicit `name` and `team_name` so the coordinator can address them directly via `SendMessage`. Each agent runs `/do-work` for one issue.

Confirm trunk-based workflow (check CLAUDE.md for `## Workflow: trunk-based`). If not found, warn: "This skill is for trunk-based repos — are you sure you want to proceed?"

Resolve the current GitHub user — the **identity filter** used throughout:

```bash
CURRENT_USER=$(gh api user --jq '.login')
```

---

## Stage 2 — Scan the backlog

Resolve the project number and owner for this repo (same pattern as the `ready` skill — query an existing issue's `projectItems`):

```bash
OWNER=$(gh repo view --json owner --jq '.owner.login')
SAMPLE_ISSUE=$(gh issue list --state open --limit 1 --json number --jq '.[0].number')
PROJECT_NUM=$(gh issue view "$SAMPLE_ISSUE" --json projectItems --jq '.projectItems[0].project.number')
```

If no open issues exist or no project is linked, ask the user for the project number directly.

Then list Ready items:

```bash
gh project item-list "$PROJECT_NUM" --owner "$OWNER" --format json | python3 -c "
import json,sys
items = json.load(sys.stdin).get('items',[])
for item in items:
    if item.get('status') == 'Ready' and item.get('content',{}).get('type') == 'Issue':
        c = item['content']
        print(f\"#{c['number']} {c['title']}\")
"
```

**Before acting on any Ready issue, check its assignees:**

```bash
gh issue view <NUMBER> --json assignees --jq '.assignees[].login'
```

| Assignee state           | Action                                                               |
| ------------------------ | -------------------------------------------------------------------- |
| Assigned to current user | Pick up — proceed                                                    |
| Unassigned               | Skip — log: "Skipping #N — no assignee. Assign to queue for pickup." |
| Assigned to someone else | Skip silently — belongs to another developer's session               |

For each Ready issue assigned to current user:

| Status      | Action                                                              |
| ----------- | ------------------------------------------------------------------- |
| Ready       | Full workflow from kick-off                                         |
| In Progress | Resume — check for active named agent or orphaned worktree          |
| Needs Human | Skip — notify user                                                  |
| Blocked     | Skip — check if blocker resolved                                    |
| Parking Lot | Skip — explicitly deferred, do not pick up without user instruction |
| Next        | Treat as Ready — user has flagged for imminent pickup               |

---

## Stage 3 — Assign and spawn named agents

For each Ready issue, spawn a **named general-purpose agent** using the `Agent` tool with `team_name` and `name` set:

```
Agent:
  team_name: "dev-team"
  name: "dev-<issue-number>"
  subagent_type: "general-purpose"
  prompt: |
    Run /do-work for issue #N.
    /do-work is the single entry point — it handles the full stage sequence
    (kick-off, plan, develop, local-review, streamline-doc for docs, push, close).
    Report blockers to the coordinator via SendMessage, don't wait silently.
```

- Set issue project status to "In Progress"
- `/do-work` carries all the flow rules — single source of truth for the workflow
- Spawn concurrently — no sequencing needed

---

## Stage 4 — Monitor

**Push approval requests:** agents request approval before pushing (`do-ship` Stage 2 gate). The coordinator's role here is defined by **project CLAUDE.md** (`## Ship policy`):

- If policy allows team-lead approval: review the incoming review summary + commit list and reply with explicit approval or change requests via `SendMessage`. Do not rubber-stamp.
- If policy requires human-only approval (no coordinator substitute): the coordinator is a **router**, not an approver. Surface the approval request to the human, forward the human's go-ahead back to the agent, and never approve unilaterally. Silence from the human is never approval — the queue holds until the human responds.

Read CLAUDE.md before the first approval request lands and surface the policy: "Per project policy I will approve directly / route to the human." So the user can correct if needed.

A pushed commit on main is effectively deployed — when in doubt, route to the human.

**Push conflict:** agent rebases and retries — up to 3 times on a busy trunk. Still failing after 3 → pause, escalate to user.

**ADR escalation:** hard stop on affected agent, notify user immediately, others continue.

**Agent stall (>30 min):** `SendMessage` to the named agent asking what's blocking. Escalate to user if unresolved.

**Issue closed:** log completion, scan for new Ready issues and assign.

---

## Stage 5 — Conflict awareness

Track recent pushes to main:

```bash
git log origin/main --oneline --since="1 hour ago"
```

If agents are working on overlapping files, send an early-warning `SendMessage` so the second agent knows it will likely need to rebase:

> "dev-42: dev-38 recently touched `src/registry.ts` — your push may need a rebase."

The actual conflict resolution mechanism is the `do-push` rebase-and-retry loop (Stage 4 above). This warning is just early notice — there is no hard sequencing or coordination protocol. Escalate only if rebase produces test failures.

---

## Rules

- **Use `TeamCreate` and named agents** — not anonymous subagents. Named agents are addressable, resumable, and visible in the shared task list.
- Never sequence pushes — concurrent by design
- ADR escalation is always a hard stop
- **Never pick up an unassigned Ready issue** — log it
- **Never pick up an issue assigned to someone else** — skip silently
- **Only pick up issues assigned to current GitHub user** (`gh api user --jq '.login'`)
- Worktrees are always siblings to the main directory: `<project>-worktrees/<issue>-<slug>`
