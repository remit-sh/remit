# Developer context

The developer is new to Rust. They are fluent in TypeScript, C#, and other mainstream languages, so general programming concepts, OO patterns, generics, async, and tooling vocabulary are all familiar — **only Rust-specific syntax and concepts need explaining**, and only when they ask.

When they ask for an explanation: be concrete, contrast with TS/C# where useful (e.g. "this is like `Option<T>` — closer to TS `T | undefined` than to C# nullable"), and avoid restating ideas they already know from other languages.

Don't pre-emptively explain Rust idioms in code reviews or while making changes. Wait for the question.

# Project

Rust workspace. Hexagonal architecture (loosely — will tighten over time).

```
crates/
├── domain/             # pure types + functions; no ports yet
└── claude-code-hook/   # inbound adapter — Claude Code PreToolUse hook
```

`claude-code-hook` reads the hook payload from stdin, calls `domain::log_input` then `domain::classify`, and signals the decision via exit code (0 = allow, 2 = deny with reason on stderr).

`domain::classify` currently always returns `Green`. Real rules to come.

# Build

```
cargo build
cargo run -p claude-code-hook   # not useful standalone — needs hook payload on stdin
```
