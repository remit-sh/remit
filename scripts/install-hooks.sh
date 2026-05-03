#!/usr/bin/env bash
# Install the remit pre-push hook.
# Creates a symlink from .git/hooks/pre-push to scripts/pre-push.
# Idempotent: safe to run multiple times.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOOKS_DIR="$REPO_ROOT/.git/hooks"

if [ ! -d "$HOOKS_DIR" ]; then
    echo "error: .git/hooks/ not found — run from within the remit repository." >&2
    exit 1
fi

HOOK_SOURCE="$SCRIPT_DIR/pre-push"
HOOK_DEST="$HOOKS_DIR/pre-push"

chmod +x "$HOOK_SOURCE"
ln -sf "$HOOK_SOURCE" "$HOOK_DEST"

echo "Installed: $HOOK_DEST -> $HOOK_SOURCE"
