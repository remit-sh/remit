---
name: do-debug
description: Switches the development environment to a target worktree — resolves the path, opens it in VS Code (reusing the current window), changes the terminal context via tmux, and starts the dev server.
---

# do-debug

Switches your active development environment to a target worktree in one command. Handles path resolution, VS Code window reuse, terminal context switching via tmux, and dev server startup.

## Triggers

`/do-debug <branch-or-worktree-name>`, "switch to <branch>", "debug <branch>", "work on <worktree>"

The branch/worktree name is required. If not provided, list available worktrees and ask.

---

## Step 1 — Resolve the Worktree Path

Find the absolute path for the given name:

```bash
TARGET="<branch-or-worktree-name>"
PATH=$(git worktree list | grep "$TARGET" | awk '{print $1}')
```

**If no match:** list all available worktrees and stop:

```bash
git worktree list
```

> "No worktree found matching `<target>`. Available worktrees above — which one did you mean?"

**If multiple matches:** show them and ask the user to be more specific.

**If match found:** confirm:

> "Found worktree: `<path>` (branch: `<branch>`)"

---

## Step 2 — Switch VS Code Window

Open the worktree in the current VS Code window using the reuse flag:

```bash
code -r "$PATH"
```

This swaps the active VS Code window to the new directory without opening a second window. If `code` is not on PATH, warn:

> "VS Code CLI (`code`) not found. Install it via VS Code: Cmd+Shift+P → 'Shell Command: Install code in PATH'"

---

## Step 3 — Detect Package Manager

Check the worktree directory for a lock file to determine the right runner:

```bash
if [ -f "$PATH/bun.lockb" ]; then RUNNER="bun"
elif [ -f "$PATH/pnpm-lock.yaml" ]; then RUNNER="pnpm"
elif [ -f "$PATH/yarn.lock" ]; then RUNNER="yarn"
else RUNNER="npm"
fi
```

Check that a `dev` script exists:

```bash
cat "$PATH/package.json" | grep '"dev"' 2>/dev/null
```

If no `dev` script: warn and ask what start command to use instead.

---

## Step 4 — Switch Terminal Context and Start Dev Server

Use `tmux send-keys` to change the current pane's directory and start the dev server — this is the only way to actually change the terminal's working directory from a skill:

```bash
tmux send-keys "cd \"$PATH\" && $RUNNER run dev" Enter
```

**If not inside tmux** (`$TMUX` is unset): fall back to printing the commands for the user to run manually:

> "Not in a tmux session — run these commands in your terminal:
>
> ````
> cd "<path>"
> <runner> run dev
> ```"
> ````

**If tmux send-keys fails** (e.g. no active pane): same fallback — print the commands.

---

## Step 5 — Summary

```
✅ Switched to <branch>.

Worktree:   <path>
VS Code:    Switched (reused window)
Terminal:   cd + <runner> run dev sent to tmux pane
Dev server: Starting...
```

---

## Rules

- Never open a second VS Code window — always use `code -r`
- Never hardcode `npm` — always detect the package manager from lock files
- If tmux isn't available, degrade gracefully — print the commands, don't fail silently
- Never assume a `dev` script exists — check `package.json` first
