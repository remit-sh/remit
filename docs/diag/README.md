# Hook payload diagnostics

Reference traces from real Claude Code sessions, captured during early design work to figure out **how a parent agent links to its subagents** purely through hook payload + env.

These files are an empirical reference. The diagnostic dumping code that produced them has been stripped from the codebase — the design questions it answered are settled. Re-run with similar instrumentation if Claude Code's hook contract changes.

## Files

| File | What it is |
|---|---|
| `team-spawn-dump.log` | Full per-call dump (env vars, process tree, raw payload) for a parent + one named team member doing one trivial Bash call. |
| `team-spawn-incoming.log` | Same trace as above in our structured one-line-per-call format. |
| `in-process-spawn-dump.log` | Same dump, but for a parent invoking a regular (in-process) `Agent` tool subagent. |
| `in-process-spawn-incoming.log` | Structured version. |
| `user-prompt-submit-dump.log` | Raw `UserPromptSubmit` stdin payloads for four root-session prompts: plain text, custom command with args, skill with args, bare custom command. See [UserPromptSubmit payload](#userpromptsubmit-payload). |

Secrets (API key, Telegram token) are redacted in the spawn dumps.

## Two subagent topologies in Claude Code

Claude Code spawns subagents in two distinct ways. They look very different at the hook level:

### In-process Agent (`Agent` tool, default flags)

- Subagent runs **inside the parent's `claude` process** — no fork.
- Subagent's tool calls fire `PreToolUse` with **the parent's `session_id`**.
- Each tool call carries a unique `agent_id` field — that's the subagent's identity.
- Lifecycle events: `SubagentStart` / `SubagentStop`.
- `CLAUDE_SESSION_ID` env var is **not set** (no fork to set it).

### Team peer-session (`Agent` with `team_name` and `run_in_background=true`, after `TeamCreate`)

- Subagent runs in a **brand-new `claude` process** with its own `session_id`.
- Tool calls fire `PreToolUse` with the **subagent's** `session_id` and **no** `agent_id`.
- The new `claude` process's argv contains `--parent-session-id <parent>`.
- Equivalently (and more easily readable): the env var `CLAUDE_SESSION_ID` is set to the **parent's** `session_id`.
- Lifecycle events: full session lifecycle — `SessionStart` / `SessionEnd` / `Stop` / `UserPromptSubmit`.

## The unified linkage rule

Both topologies collapse to a single, platform-agnostic rule the domain can use:

```
parent_session_id = CLAUDE_SESSION_ID env var (if set, and ≠ self)
                  OR None (top-level / in-process)
```

Then for any scope/inheritance lookup:

```
key = parent_session_id.unwrap_or(session_id)
```

| Topology | `session_id` | `CLAUDE_SESSION_ID` | resolved key |
|---|---|---|---|
| Top-level | A | unset | A |
| In-process Agent | A (parent's) | unset | A |
| Team peer-session | B (new) | A (parent's) | A |
| Nested (team member spawning own in-process subagent) | B | A (root, unchanged) | A |

The nested case resolves to **root**, not immediate parent — because the env var is fork-time inherited and in-process spawns don't fork. For a single-level "child inherits parent" model this is exactly what you want. Per-level scope (different scope at each depth) would need additional lineage tracking; not in v1.

## What's *not* exposed by the hook contract

- `SubagentStart` / `SubagentStop` fire only for **in-process** subagents — not team members.
- The hook payload itself contains **no parent reference** for team members. The link only exists via env / process argv.
- `agent_id` (in-process spawn ID) is only present for in-process subagents and is unique per spawn but does not encode lineage between nested spawns.

## UserPromptSubmit payload

Captured for #60 ([TN-2026-017](../tn/TN-2026-017-extract-scope-from-text.md)) with Claude Code 2.1.322 in headless mode (`claude -p`), via a project-local hook dumping stdin.

- Fields: `session_id`, `transcript_path`, `cwd`, `prompt_id`, `permission_mode`, `hook_event_name`, `prompt`.
- No `agent_id`, no `agent_type`. `CLAUDE_SESSION_ID` unset for a root session.
- **Slash commands arrive raw.** `prompt` is exactly what was typed (`/diag-cmd 42`, `/diag-skill 42`, `/diag-cmd`). Command and skill bodies are not expanded into the payload, though the model did receive the expanded body.
- **A nested session is indistinguishable from a top-level one.** `claude -p` launched from inside an agent's Bash tool and one launched from a clean shell present the same hook env: `CLAUDE_SESSION_ID` unset, `CLAUDECODE=1`, `CLAUDE_CODE_ENTRYPOINT=sdk-cli`, and `CLAUDE_CODE_SESSION_ID` / `CLAUDE_PID` rewritten to the nested session's own values. Checked by dumping those vars from the hook in both launches; env dump not kept in the log file.
- Not captured: interactive (TUI) mode, built-in commands (`/clear`, `/cost`), peer-process team sessions.
