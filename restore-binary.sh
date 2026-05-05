#!/usr/bin/env sh
# Restore the main build into ~/.local/bin/remit-claude-code-hook by
# finding the main worktree via git and running its swap-binary.sh.
# Run this from any worktree — no need to cd back to main yourself.

set -eu

MAIN=$(git worktree list 2>/dev/null | awk '$NF == "[main]" { print $1; exit }')

if [ -z "${MAIN:-}" ]; then
    echo "restore-binary: could not find a worktree on branch main" >&2
    echo "restore-binary: run 'git worktree list' to inspect" >&2
    exit 1
fi

if [ ! -x "$MAIN/swap-binary.sh" ]; then
    echo "restore-binary: $MAIN/swap-binary.sh missing or not executable" >&2
    exit 1
fi

exec "$MAIN/swap-binary.sh"
