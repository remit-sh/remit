---
name: ready
description: Mark the current issue as ready — sets the GitHub Project status to "Ready" so the team picks it up.
---

# ready

Sets the current issue's project status to "Ready" for team pickup. Run from any session when you're done steering and want the team to take over.

## Triggers

`/ready`, "this is ready", "that one's ready", "set to ready", "mark as ready", "ready for team", "queue this up"

---

## Step 1 — Identify the Issue

Infer from branch name:

```bash
BRANCH=$(git branch --show-current)
ISSUE_NUM=$(echo "$BRANCH" | grep -oE '^[0-9]+')
gh issue view "$ISSUE_NUM" --json number,title,state,projectItems 2>/dev/null
```

If no issue found: ask the user for the issue number.
If issue is closed: warn and stop.

---

## Step 2 — Set Project Status to "Ready"

```bash
OWNER="<org-or-user>"
PROJECT_NUM=$(gh issue view "$ISSUE_NUM" --json projectItems --jq '.projectItems[0].project.number')
ITEM_ID=$(gh issue view "$ISSUE_NUM" --json projectItems --jq '.projectItems[0].id')
PROJECT_ID=$(gh project view "$PROJECT_NUM" --owner "$OWNER" --format json | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")

# Get Status field ID and "Ready" option ID
gh project field-list "$PROJECT_NUM" --owner "$OWNER" --format json | python3 -c "
import json,sys
fields = json.load(sys.stdin).get('fields',[])
for f in fields:
    if f.get('name') == 'Status':
        print('FIELD_ID=' + f['id'])
        for opt in f.get('options',[]):
            if opt['name'] == 'Ready':
                print('OPTION_ID=' + opt['id'])
"

gh project item-edit --project-id "$PROJECT_ID" --id "$ITEM_ID" --field-id "$STATUS_FIELD_ID" --single-select-option-id "$OPTION_ID"
```

If the issue is not yet in a project, add it first:

```bash
gh project item-add "$PROJECT_NUM" --owner "$OWNER" --url "$(gh issue view "$ISSUE_NUM" --json url --jq '.url')"
```

---

## Step 3 — Confirm

Show current state:

```bash
gh issue view "$ISSUE_NUM" --json number,title,projectItems \
  --jq '"#\(.number) \(.title) — status: \(.projectItems[0].status.name)"'
```

Report:

```
#<N> "<title>" is ready.

Project status: Ready

The team coordinator will find this on its next backlog scan.
```

---

## Rules

- Never modify the issue body or close the issue
- If the issue is already "Ready", report it and stop
