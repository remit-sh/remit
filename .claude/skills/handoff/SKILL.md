---
name: handoff
description: Compact the current conversation into a handoff document for another agent session to pick up. Use when ending a session mid-work, switching context, or preparing for another agent to continue.
argument-hint: 'What will the next session focus on?'
---

# Handoff

Write a handoff document summarising the current conversation so a fresh agent can continue the work. Save it to a path produced by `mktemp -t handoff-XXXXXX.md` (read the file before writing).

## What to include

- **Current state** — where work stands right now (in one paragraph)
- **Key decisions made** — only non-obvious ones that aren't captured elsewhere
- **Open questions / blockers** — anything unresolved
- **Next steps** — what the next session should do first
- **Suggested skills** — which skills to invoke, if any

## What NOT to include

Do not duplicate content already captured in other artefacts. Reference by path or URL instead:

- PRDs, specs, plans → link the file
- Issues → link the issue number
- Commits → reference the SHA
- Diffs → reference the branch

## Rules

- Keep it under 50 lines. A handoff that requires reading is a bad handoff.
- Write for a cold reader — no "as we discussed" without stating what was discussed.
- If the user passed arguments, treat them as a description of what the next session will focus on and tailor accordingly.
