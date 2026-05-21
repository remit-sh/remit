#!/usr/bin/env bash
# Claude Code PreToolUse hook: block `git push` unless approval marker is present and pinned to HEAD.
# Reads tool-call JSON on stdin. Exits 2 to block; exits 0 to allow.

set -euo pipefail

input=$(cat)
command=$(echo "$input" | jq -r '.tool_input.command // empty' 2>/dev/null || true)

if ! [[ "$command" =~ (^|[[:space:]\;\&\(\|])git[[:space:]]+([^\;\&\|\(\)]*[[:space:]])?push([[:space:]]|$|\;|\&|\)) ]]; then
    exit 0
fi

repo_root="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"
marker="$repo_root/.agent-state/last-approval.json"

if [[ ! -f "$marker" ]]; then
    echo "Approval marker missing ($marker). Run /do-ship and get human approval before pushing." >&2
    exit 2
fi

marker_sha=$(jq -r '.head_sha // empty' "$marker" 2>/dev/null || true)
head_sha=$(git -C "$repo_root" rev-parse HEAD 2>/dev/null || true)

if [[ -z "$marker_sha" || -z "$head_sha" ]]; then
    echo "Approval marker malformed or HEAD unresolved. Re-run /do-ship." >&2
    exit 2
fi

if [[ "$marker_sha" != "$head_sha" ]]; then
    echo "Approval marker is stale. Marker SHA: $marker_sha, current HEAD: $head_sha. Re-run /do-ship." >&2
    exit 2
fi

exit 0
