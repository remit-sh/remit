#!/usr/bin/env sh
# Build claude-code-hook from this worktree and swap it in at the stable
# path Claude Code points at: ~/.local/bin/remit-claude-code-hook.
# Records active branch + worktree in a sidecar file; cat the sidecar to
# see what's running. Use restore-binary.sh to swap back to the main build.

set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
DEST="${HOME}/.local/bin/remit-claude-code-hook"
SIDECAR="${DEST}.source"
SRC="$HERE/target/debug/claude-code-hook"

(cd "$HERE" && cargo build -p claude-code-hook)

if [ ! -f "$SRC" ]; then
    echo "swap-binary: build did not produce $SRC" >&2
    exit 1
fi

mkdir -p "$(dirname "$DEST")"
cp "$SRC" "$DEST.tmp"
mv "$DEST.tmp" "$DEST"

BRANCH=$(git -C "$HERE" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
WORKTREE=$(git -C "$HERE" rev-parse --show-toplevel 2>/dev/null || echo "$HERE")
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

printf 'branch:   %s\nworktree: %s\nbuilt:    %s\n' "$BRANCH" "$WORKTREE" "$TIMESTAMP" > "$SIDECAR"

echo "swapped:  $DEST"
echo "active:   $BRANCH @ $WORKTREE"
