#!/usr/bin/env bash
# Install the remit pre-push hook.
# Creates a relative symlink so it survives repo-directory renames and works
# from any linked worktree's hook invocation.
# Idempotent: safe to run multiple times.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOKS_DIR="$(git -C "$SCRIPT_DIR" rev-parse --git-common-dir)/hooks"

mkdir -p "$HOOKS_DIR"
chmod +x "$SCRIPT_DIR/pre-push"

# Relative target resolves from .git/hooks/ → <repo>/scripts/pre-push.
ln -sf ../../scripts/pre-push "$HOOKS_DIR/pre-push"

echo "Installed: $HOOKS_DIR/pre-push -> ../../scripts/pre-push"
