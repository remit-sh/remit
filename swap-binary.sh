#!/usr/bin/env sh
# Build the Remit binaries from this worktree and swap them in at the stable
# paths Claude Code points at: ~/.local/bin/remit-claude-code-hook,
# ~/.local/bin/remit-prompter, and ~/.local/bin/remit. Records active branch
# + worktree in a sidecar file; cat the sidecar to see what's running. Use
# restore-binary.sh to swap back to the main build.

set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
HOOK_DEST="${HOME}/.local/bin/remit-claude-code-hook"
PROMPTER_DEST="${HOME}/.local/bin/remit-prompter"
REMIT_DEST="${HOME}/.local/bin/remit"
SIDECAR="${HOOK_DEST}.source"
HOOK_SRC="$HERE/target/debug/claude-code-hook"
PROMPTER_SRC="$HERE/swift/.build/debug/RemitPrompter"
REMIT_SRC="$HERE/target/debug/remit"

(cd "$HERE" && cargo build -p claude-code-hook -p remit-cli)
(cd "$HERE/swift" && swift build)

if [ ! -f "$HOOK_SRC" ]; then
    echo "swap-binary: build did not produce $HOOK_SRC" >&2
    exit 1
fi
if [ ! -f "$PROMPTER_SRC" ]; then
    echo "swap-binary: build did not produce $PROMPTER_SRC" >&2
    exit 1
fi
if [ ! -f "$REMIT_SRC" ]; then
    echo "swap-binary: build did not produce $REMIT_SRC" >&2
    exit 1
fi

mkdir -p "$(dirname "$HOOK_DEST")"
cp "$HOOK_SRC" "$HOOK_DEST.tmp"
mv "$HOOK_DEST.tmp" "$HOOK_DEST"
cp "$PROMPTER_SRC" "$PROMPTER_DEST.tmp"
mv "$PROMPTER_DEST.tmp" "$PROMPTER_DEST"
cp "$REMIT_SRC" "$REMIT_DEST.tmp"
mv "$REMIT_DEST.tmp" "$REMIT_DEST"

BRANCH=$(git -C "$HERE" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
WORKTREE=$(git -C "$HERE" rev-parse --show-toplevel 2>/dev/null || echo "$HERE")
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

printf 'branch:   %s\nworktree: %s\nbuilt:    %s\n' "$BRANCH" "$WORKTREE" "$TIMESTAMP" > "$SIDECAR"

echo "swapped:  $HOOK_DEST"
echo "          $PROMPTER_DEST"
echo "          $REMIT_DEST"
echo "active:   $BRANCH @ $WORKTREE"
