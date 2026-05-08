# Remit

**Semantic Firewall for AI Agent Sessions**

Remit keeps AI agents within their **Scope** — the task they're trying to do. Every tool call gets a **Verdict**: Green allows, Red blocks. Local-first, no telemetry.

[![License: BSL 1.1](https://img.shields.io/badge/License-BSL_1.1-blue.svg)](LICENSE)

> **Early-stage software — fresh rewrite, not yet production.**

## Status

Trunk-based rewrite in progress. See [`docs/tn/`](docs/tn/) for the working surface and [`docs/decisions/`](docs/decisions/) for ratified ADRs and CDRs.

## Layout

```
crates/
├── domain/              # ports, types, services — pure
├── remit-config/        # ~/.remit/config.toml loader
├── file-logger/         # outbound adapter — append-only NDJSON log
├── file-scope-store/    # outbound adapter — JSON-per-session scope store
└── claude-code-hook/    # inbound adapter — Claude Code PreToolUse hook
```

## Build

```sh
cargo build
just check    # fmt + clippy + tests (mirrors the pre-push hook)
```

After cloning, install the **git** pre-push hook:

```sh
./scripts/install-git-hooks.sh
```

## Setup

### 1. Wire the Claude Code hook

Add this PreToolUse entry to `~/.claude/settings.json` (merge with any existing `hooks` block — Claude Code doesn't expand `~`, so use the absolute path):

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/Users/<your-username>/.local/bin/remit-claude-code-hook"
          }
        ]
      }
    ]
  }
}
```

The path is stable; the binary it points at changes via the scripts below.

### 2. Install the binary

```sh
./swap-binary.sh
```

Builds this worktree and copies the binary to `~/.local/bin/remit-claude-code-hook`. Writes a sidecar `~/.local/bin/remit-claude-code-hook.source` recording active branch + worktree + build timestamp.

## Local development

| Command | What it does |
|---|---|
| `./swap-binary.sh` (any worktree) | Build + install this worktree's binary. Use when debugging a feature branch. |
| `./restore-binary.sh` (any worktree) | Find main via `git worktree list`, build + install main's binary. Reverts to default. |
| `./toggle-dev-bypass.sh` | Flip `dev_bypass` in `<remit_home>/config.toml`. Hook still classifies every call but exits 0 on Red with a `[dev-bypass] would have denied: ...` line on stderr. |

See [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md) for the full configuration reference (env vars, `config.toml` schema, on-disk layout).

Check what's running:

```sh
cat ~/.local/bin/remit-claude-code-hook.source
```

## License

Business Source License 1.1. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
