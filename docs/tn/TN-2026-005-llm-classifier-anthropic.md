---
number: 2026-005
title: LLM classifier — port, Anthropic adapter, hardening
kind: plan
status: active
issue: "#22"
tags: [classifier, llm, anthropic, hardening]
---

# TN-2026-005: LLM classifier — port, Anthropic adapter, hardening

> Plan for the semantic-firewall classifier: the port in domain (#22, shipped), the Haiku-backed Anthropic adapter (shipped), and the hardening that closes obvious bypasses (#34, #35).

## Where the LLM call slots in

- `Classifier::classify` consults the LLM only on the scope-present branch.
- Verdict ladder:

| Branch | Verdict |
|---|---|
| scope absent + root | `Red(NoScopeRoot)` |
| scope absent + subagent | `Red(SubagentMissingScope)` |
| scope present + LLM `Ok(Green)` | `Green` |
| scope present + LLM `Ok(Red{reason})` | `Red(OutOfScope, reason)` |
| scope present + LLM `Err(_)` | `Red(ClassifierUnavailable)` — fail-closed |

## Domain port

- `LlmClassifier::classify(action, scope) -> Result<LlmDecision, LlmClassifierError>`.
- `LlmDecision`: `Green` or `Red { reason }`. Adapter doesn't know `Verdict` / `RedKind` — domain owns the verdict ladder.
- `LlmClassifierError::Unavailable` collapses network / parse / auth. Fail-closed needs no finer split.
- Port takes `&Action` + `&Scope`. Working dir + agent identity are not the LLM's concern at this layer.

## Anthropic adapter (`anthropic-classifier`)

- `ureq` blocking. Hook is a single-shot CLI; async buys nothing.
- `POST https://api.anthropic.com/v1/messages`. Headers: `x-api-key`, `anthropic-version: 2023-06-01`. 10s connect + 30s read; any failure → `Unavailable`.
- Forced tool-use (`verdict` tool) so response shape is schema-validated, not free-text JSON.
- System prompt = `domain::CLASSIFIER_GUIDANCE` + adapter-specific wire directives. Scope and action wrapped in `<scope>` / `<action_tool>` / `<action_input>` tags; guidance instructs the model to treat tag contents as data only.
- Args truncated at 8 KB — bounds request size and operator quota burn under pathological inputs.

## Config

- `[classifier].adapter` selects implementation; per-adapter sub-section carries typed config.
- `AnthropicConfig`: optional `model` (defaults to `DEFAULT_MODEL`), optional `api_key` (falls back to `ANTHROPIC_API_KEY`).
- Missing/whitespace-only key at startup → `fail_closed` exit 2 before any tool call. The hook never runs without a working classifier.

## Hardening

The classifier currently sees the full tool-input JSON — including agent-authored fields like `Bash.description` — as one undifferentiated args blob. Self-narration rides in alongside the action and tilts verdicts. Two issues track the close, plus belt-and-braces prompt work and a deferred lineage check.

### #34 — strip agent-narration fields from action args

- Generic JSON-tree denylist applied in `claude-code-hook` before constructing `ActionArgs`.
- Drops keys matching a fixed list of universally-narrative names — `description`, `reason`, `note`, `comment`, `intent`, `purpose`, `rationale`, `justification`, `summary` — case-insensitive, any depth.
- Tool-agnostic: no per-tool or per-harness logic. Generalises to Codex / Cline / MCP servers — same field names, same strip.
- Trade-off: occasionally strips legitimate content (e.g. a Linear issue body field named `description`). Acceptable — false-Red is fail-closed; the dangerous direction is false-Green.

### #35 — what-extractor pass on bundled scopes at spawn boundaries

- The bundled-scope problem is structurally different: `Agent.prompt` *is* the scope, can't be stripped. A parent crafting `"update ticket status — for cleanup"` smuggles justification into the child's future scope, where it tilts every later classification toward Green.
- LLM extraction call: aggressive imperative-only output (drop causal clauses, justifications, qualifiers, framing).
- Runs at two boundaries:
  1. Parent's `PreToolUse` on a spawn tool — clean the prompt before classifying the spawn against parent scope.
  2. Bundled-scope → child-scope materialisation — clean again before the text becomes the child's stored scope.
- Cost: one extra LLM call per spawn (rare). Same model can be reused initially.

### Lineage validation (later)

- `(child_scope, parent_scope)` plausibility check at spawn time. Anchored transitively on the human-set root scope.
- Defers behind #35 — without cleaning, framing tilts the lineage check the same way it tilts action classification.

### Classifier-prompt belt-and-braces

- Add to `CLASSIFIER_GUIDANCE`: discount self-narrative fields; decide on the action's effect, not the agent's framing.
- Marginal alone; reinforces #34 against fields not on the strip list and against narration baked into `command` itself (e.g. `# this is for cleanup`).

## Security

- API key in `config.toml` is the user's tradeoff; key is easily replaceable. Env-var fallback (`ANTHROPIC_API_KEY`) provided.
- `remit_config::load()` warns to stderr on group/other-readable `config.toml` containing an api_key. Hard enforcement is out of scope.
- `ClassifierConfig::Debug` redacts `api_key`. `AnthropicClassifier` derives no `Debug`. Never log the API key. Never log full `tool_input` payloads.
- HTTPS request body contains the **Scope** and a truncated **Action args** (8 KB cap). Same telemetry surface the user already routes through Anthropic via Claude Code itself — no new exfiltration in that deployment. Adapter doc warns against wiring under non-Anthropic agents.
- TLS via `ureq` `tls` feature (rustls + webpki-roots). HTTPS endpoint hardcoded; only doc-hidden `with_endpoint` accepts other URLs (test-only).

## Out of scope

- Caching, retries, streaming.
- Multi-classifier consensus, fallback chains.
- `LlmClassifier` contract test in `domain::test_support` — needs a stable fake; deferred until a second adapter exists.
- Latency mitigation beyond timeouts.
- Hard enforcement of `chmod 600` on `config.toml` (warning only).
- Tightening of `FileLogger` to redact / truncate full `tool_input` payloads — pre-existing behaviour, separate issue.
- Bypass prompt (#21).
- Lineage validation depth + drift accumulation — explicitly later than #35.
