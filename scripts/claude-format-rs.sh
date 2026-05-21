#!/usr/bin/env bash
# Claude Code PostToolUse hook: format Rust files on Edit/Write.
# Reads tool-call JSON on stdin, runs rustfmt on the changed file if it's .rs.
# Silent on success and on rustfmt failure (transient mid-edit syntax errors
# are normal; the pre-push hook will catch persistent fmt issues).

set -euo pipefail

file_path=$(jq -r '.tool_input.file_path // empty' 2>/dev/null || true)

[[ -z "$file_path" || "$file_path" != *.rs ]] && exit 0
[[ ! -f "$file_path" ]] && exit 0

rustfmt --edition 2024 "$file_path" >/dev/null 2>&1 || true
exit 0
