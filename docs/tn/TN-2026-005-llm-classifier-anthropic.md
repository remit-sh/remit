---
number: 2026-005
title: LLM classifier port + Anthropic adapter
kind: plan
status: active
issue: "#22"
tags: [classifier, llm, anthropic, mvp]
---

# TN-2026-005: LLM classifier port + Anthropic adapter

> Replaces the always-Green stub in `Classifier::classify` with a real Haiku-backed semantic-firewall call. First slice of #18.

## Goal

- Every `PreToolUse` with a present **Scope** is classified by an LLM, not a stub.
- Port lives in domain. Adapter lives in its own crate. Composition root wires them.
- Fail-closed on any classifier or network error.
- Model + API key configurable via `~/.remit/config.toml`. Key falls back to `ANTHROPIC_API_KEY` env var when absent.

## Where the LLM call slots in

`Classifier::classify` today:

| Branch | Verdict |
|---|---|
| scope present | `Green` (stub) |
| scope absent + root | `Red(NoScopeRoot)` |
| scope absent + subagent | `Red(SubagentMissingScope)` |

After:

| Branch | Verdict |
|---|---|
| scope present + LLM `Ok(Green)` | `Green` |
| scope present + LLM `Ok(Red{reason})` | `Red(OutOfScope, reason)` |
| scope present + LLM `Err(_)` | `Red(ClassifierUnavailable, reason)` |
| scope absent + root | unchanged |
| scope absent + subagent | unchanged |

The scope-absent ladder doesn't change — those Reds happen before any LLM call.

## Domain port

```rust
pub trait LlmClassifier {
    fn classify(&self, action: &Action, scope: &Scope)
        -> Result<LlmDecision, LlmClassifierError>;
}

pub enum LlmDecision {
    Green,
    Red { reason: String },
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum LlmClassifierError {
    #[error("LLM classifier unavailable: {0}")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
}
```

- Adapter doesn't know `Verdict` / `RedKind` — domain owns the verdict ladder.
- Single error variant in MVP. Network/parse/auth all collapse to `Unavailable`. Fail-closed needs no finer split.
- Port takes `&Action` + `&Scope`. Working dir + agent identity are not the LLM's concern at this slice.

## RedKind additions

```rust
RedKind::OutOfScope          // LLM judged the action doesn't serve the scope
RedKind::ClassifierUnavailable // any LLM-side failure (fail-closed)
```

## Classifier changes

`Classifier` gains a second type parameter:

```rust
pub struct Classifier<L: ClassifierLogger, C: LlmClassifier> {
    logger: L,
    llm: C,
}
```

`classify` orchestrates: log → resolve verdict ladder → only call `llm` on the scope-present branch. Mapping:

- `Ok(LlmDecision::Green)` → `Verdict::Green`
- `Ok(LlmDecision::Red{reason})` → `Verdict::Red { kind: OutOfScope, reason }`
- `Err(LlmClassifierError::Unavailable(e))` → `Verdict::Red { kind: ClassifierUnavailable, reason: format!("classifier unavailable: {e}") }`

`process_action` signature gains a `C: LlmClassifier` param (passes through). The `NoScopeRoot → ScopeSetupCancelled` translation in `process_action` is unchanged.

## Adapter — `anthropic-classifier`

New crate at `crates/anthropic-classifier/`. Naming: `<tech>-<role>` matches `osascript-prompter`, `file-logger`. Future siblings: `llama-classifier`, `openai-classifier`.

### Surface

```rust
pub struct AnthropicClassifier {
    api_key: String,
    model: String,
}

impl AnthropicClassifier {
    pub fn new(api_key: String, model: String) -> Self;
}

impl LlmClassifier for AnthropicClassifier { ... }
```

### HTTP

- `ureq` blocking client. No async runtime. The hook is a single-shot CLI — async buys nothing.
- Endpoint: `POST https://api.anthropic.com/v1/messages`.
- Headers: `x-api-key`, `anthropic-version: 2023-06-01`, `content-type: application/json`.
- Timeout: 10s connect + 30s read. Any failure → `Unavailable`.

### Prompt

System prompt instructs Haiku to return strict JSON: `{"verdict":"green"}` or `{"verdict":"red","reason":"..."}`. User message carries the **Scope** and the **Action** (`tool_name` + `tool_input`).

- Adapter parses JSON from the assistant's first text block.
- Parse failure → `Unavailable`.
- `verdict == "green"` → `LlmDecision::Green`.
- `verdict == "red"` with non-empty `reason` → `LlmDecision::Red { reason }`.
- Anything else → `Unavailable`.

Prompt content lives in a `const` in the adapter; iteration on prompt quality is out of scope for this TN.

### Cargo

```toml
[dependencies]
domain = { path = "../domain" }
ureq = { version = "2", default-features = false, features = ["tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## Config

Tagged-union shape: `[classifier].adapter` selects the adapter; per-adapter sub-sections carry typed config.

```toml
dev_bypass = false

[classifier]
adapter = "anthropic"   # default; explicit for clarity

[classifier.anthropic]
model = "claude-haiku-4-5"   # optional, defaults to anthropic-classifier::DEFAULT_MODEL
api_key = "sk-ant-..."        # optional, falls back to ANTHROPIC_API_KEY
```

```rust
pub struct ClassifierConfig {
    pub adapter: ClassifierAdapter,    // enum, defaults to Anthropic
    pub anthropic: AnthropicConfig,    // typed sub-section per adapter
}

pub enum ClassifierAdapter { Anthropic /* future: Llama, OpenAI, ... */ }

pub struct AnthropicConfig {
    pub model: Option<String>,
    pub api_key: Option<String>,
}
```

- New adapter = new `ClassifierAdapter` variant + new field on `ClassifierConfig` + new `from_config` on the adapter crate + new arm in the host's `build_llm_classifier` match. The exhaustive match means missing the host arm is a compile error.
- `AnthropicConfig` has a redacting `Debug` impl so `dbg!(cfg)` can't leak the key.
- `DEFAULT_MODEL` and `API_KEY_ENV_VAR` (`"ANTHROPIC_API_KEY"`) are public constants on `anthropic-classifier`. Each future adapter owns its own equivalents.

## Composition root wiring (`claude-code-hook`)

```rust
fn build_llm_classifier(cfg: &remit_config::RemitConfig) -> AnthropicClassifier {
    match cfg.classifier.adapter {
        ClassifierAdapter::Anthropic => fail_closed(
            "classifier",
            AnthropicClassifier::from_config(
                cfg.classifier.anthropic.api_key.clone(),
                cfg.classifier.anthropic.model.clone(),
            ),
        ),
    }
}
```

`AnthropicClassifier::from_config(api_key, model)` does the resolution: explicit `api_key` wins; `None` falls back to `ANTHROPIC_API_KEY`; whitespace-only is treated as missing → `ConfigError::MissingApiKey`. Same shape for `model` against `DEFAULT_MODEL`.

Missing config that prevents construction → `fail_closed` exit 2 at startup, **before** any tool call. The hook never runs without a working classifier: deny everything by default.

## Test strategy

TDD throughout — failing test first, then implementation.

### Domain

- New `tests/classifier.rs` cases:
  - scope present + LLM Green → `Verdict::Green`
  - scope present + LLM Red → `Verdict::Red(OutOfScope, reason)`
  - scope present + LLM Err → `Verdict::Red(ClassifierUnavailable, _)`
  - scope-absent paths unchanged
- New `FakeLlmClassifier` in `tests/common/mod.rs` — outcomes: `green`, `red(reason)`, `unavailable`. Pinned call counts where useful.
- Update `tests/process_action.rs` to thread the fake LLM through.

### Adapter unit tests

- Request-body builder: scope + action → expected JSON shape.
- Response parser: green / red+reason / malformed JSON / non-JSON / wrong fields → expected `LlmDecision` or `Unavailable`.
- All using fixture strings; no live HTTP.

### Live integration

- Manual smoke test against the real Anthropic API is enough for this slice. Two cases against `claude-haiku-4-5`: scope "develop the front-end history component" + `Bash: ls` → Green; same scope + `Bash: cat /etc/passwd` → Red.
- No automated live test in CI yet — flakiness and cost outweigh the value at this stage.

### Config

- `parses_classifier_section` (full + partial + empty).
- `rejects_unknown_classifier_field` (`deny_unknown_fields` belt).

## Security

- API key in `config.toml` is the user's tradeoff (key is easily replaceable, see issue thread). Env-var fallback (`ANTHROPIC_API_KEY`) is provided.
- `remit_config::load()` warns to stderr when `~/.remit/config.toml` is group/other-readable and contains an api_key. Hard enforcement (refusing to load) is out of scope.
- `ClassifierConfig::Debug` redacts `api_key` so a careless `dbg!(cfg)` can't leak the key. `AnthropicClassifier` derives no `Debug`.
- Never log the API key. Never log full `tool_input` payloads beyond what `FileLogger::log_incoming` already records (pre-existing behaviour; tightening tracked separately).
- The HTTPS request body contains the **Scope** and a truncated **Action args** (8 KB cap). User-controlled fields are wrapped in `<scope>` / `<action_tool>` / `<action_input>` tags and the system prompt instructs Haiku to treat tag contents as data only — a cheap defense against prompt injection from the agent's own tool input.
- The data sent matches what the user already routes through Anthropic via Claude Code itself — no new exfiltration surface in that deployment. Adapter doc comment warns against wiring under non-Anthropic agents.
- TLS via `ureq` `tls` feature (rustls + webpki-roots). HTTPS endpoint is hardcoded; only doc-hidden `with_endpoint` (used by the integration test) accepts other URLs.

## Out of scope

- Hardened prompt-injection defences beyond tag wrapping + system-prompt framing. Tracked separately.
- Caching, retries, streaming.
- Multi-classifier consensus, fallback chains.
- A `LlmClassifier` contract test in `domain::test_support` — needs a stable fake, deferred until a second adapter exists.
- Latency mitigation. Each `PreToolUse` adds one Haiku round-trip. Acceptable for MVP.
- Hard enforcement of `chmod 600` on `config.toml` (warning only).
- Tightening of `FileLogger` to redact / truncate full `tool_input` payloads — pre-existing behaviour, separate issue.
- Bypass prompt (#21).

## Implementation order

1. Add `LlmClassifier`, `LlmDecision`, `LlmClassifierError`, new `RedKind` variants in `domain/src/lib.rs`.
2. Add `FakeLlmClassifier` in `tests/common/mod.rs`. Write failing classifier tests.
3. Update `Classifier` to take + use the port. Make tests pass.
4. Update `process_action` signature. Update `tests/process_action.rs`.
5. Create `anthropic-classifier` crate. Add to workspace members.
6. Adapter: write request/response unit tests first, then implementation.
7. Extend `remit-config` with `ClassifierConfig`. Tests.
8. Wire composition root. Update `claude-code-hook` `Cargo.toml` + `main.rs`.
9. Run `just check`. Iterate until green.
10. Manual live test with real key.
