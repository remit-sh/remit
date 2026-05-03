# Remit

**Semantic Firewall for AI Agent Sessions**

Remit keeps AI agents within their **Scope**. A local-first semantic firewall that checks every tool call against the session's declared **Scope**.

[![License: BSL 1.1](https://img.shields.io/badge/License-BSL_1.1-blue.svg)](LICENSE)

> **Early-stage software — fresh rewrite, not yet production.**

## Status

Trunk-based rewrite in progress. See [`docs/tn/`](docs/tn/) for the current working surface and [`docs/decisions/`](docs/decisions/) for ratified architectural and convention decisions.

## Build

```sh
cargo build
just check    # fmt + clippy + tests
```

After cloning, install the pre-push hook:

```sh
./scripts/install-hooks.sh
```

## Layout

```
crates/
├── domain/             # pure types + functions
└── claude-code-hook/   # inbound adapter — Claude Code PreToolUse hook
```

## License

Business Source License 1.1. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
