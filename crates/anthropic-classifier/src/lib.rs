//! Anthropic Messages API adapter for the [`LlmClassifier`] port.
//!
//! Sends the agent's [`Scope`] and the full [`Action`] (tool name + tool input)
//! to `api.anthropic.com` over HTTPS. The body is the same data the user is
//! already routing through Anthropic via Claude Code itself, so this adapter
//! adds no new exfiltration surface in that deployment.
//!
//! Do **not** wire this adapter under agents whose telemetry policy doesn't
//! already include Anthropic (e.g. local-only or non-Anthropic-vendor agents).
//!
//! Uses Anthropic's tool-use mechanism with a forced tool per operation
//! (`verdict` for classification, `extract_scope` for scope extraction). The
//! model must emit a schema-validated tool call; no free-form JSON parsing.

use std::time::Duration;

use domain::{
    Action, ClassificationContext, ClassifierOutcome, LlmCallStats, LlmClassifier,
    LlmClassifierError, RawVerdict, RedKind, SCOPE_EXTRACTION_GUIDANCE, Scope, StandingGuidance,
    Verdict, compose_trusted_system_prompt,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 256;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const VERDICT_TOOL_NAME: &str = "verdict";
const EXTRACT_SCOPE_TOOL_NAME: &str = "extract_scope";
/// Output budget for one extraction. Bounds the extracted scope well below
/// the 8 KiB [`Scope`] cap; a response cut off here is refused, not stored.
const EXTRACTION_MAX_TOKENS: u32 = 512;

/// Default model id used when the host doesn't override it.
pub const DEFAULT_MODEL: &str = "claude-haiku-4-5";
/// Environment variable consulted by [`AnthropicClassifier::from_config`] when
/// no explicit `api_key` is provided.
pub const API_KEY_ENV_VAR: &str = "ANTHROPIC_API_KEY";

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum ConfigError {
    #[error(
        "Anthropic API key not provided; pass one explicitly or set the {API_KEY_ENV_VAR} env var"
    )]
    MissingApiKey,
}

/// Cap on the bytes of `action.args` we ship to the API. Defends against a
/// pathological tool input (e.g. a multi-MB heredoc) blowing up the request,
/// burning operator API quota, and amplifying upstream timeouts.
const MAX_ARGS_BYTES: usize = 8 * 1024;

/// Wire-format directives layered on top of the trusted system prompt assembled
/// by [`compose_trusted_system_prompt`]. Tells the model where the data lives
/// and how to emit the verdict. Anthropic-specific.
const WIRE_DIRECTIVES: &str = "\
The SCOPE arrives wrapped in <scope> tags; the action's tool name and input arrive in <action_tool> and <action_input>. \
Decide and emit the verdict by calling the `verdict` tool — do not reply with prose.";

/// Cap on the bytes of text accepted for scope extraction. Larger text is
/// refused, never truncated: a cut-off tail could hold the limits that narrow
/// the task, and losing them would widen the extracted scope.
const MAX_EXTRACTION_TEXT_BYTES: usize = 32 * 1024;

/// Wire-format directives layered on top of [`SCOPE_EXTRACTION_GUIDANCE`].
/// Anthropic-specific.
const EXTRACTION_WIRE_DIRECTIVES: &str = "\
The TEXT arrives wrapped in <text> tags. \
Report the result by calling the `extract_scope` tool — do not reply with prose.";

/// The tags that wrap data in this adapter's requests (see
/// [`user_message`], [`build_extraction_request`]). Text carrying one could
/// restructure a request, so extraction refuses it on the way in (`text`)
/// and on the way out (the extracted scope). Hardening for the extraction
/// path only, not a general injection defence.
const RESERVED_TAGS: &[&str] = &[
    "<text>",
    "</text>",
    "<scope>",
    "</scope>",
    "<action_tool>",
    "</action_tool>",
    "<action_input>",
    "</action_input>",
];

/// Case- and whitespace-insensitive: `</Scope >` and `< /scope>` both match.
fn carries_reserved_tag(text: &str) -> bool {
    let squashed: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    RESERVED_TAGS.iter().any(|t| squashed.contains(t))
}

fn system_prompt(standing: &StandingGuidance) -> String {
    format!(
        "{}\n\n{WIRE_DIRECTIVES}",
        compose_trusted_system_prompt(standing)
    )
}

/// Anthropic Messages API adapter for the [`LlmClassifier`] port.
/// Talks to `https://api.anthropic.com/v1/messages` over HTTPS.
pub struct AnthropicClassifier {
    api_key: String,
    model: String,
    endpoint: String,
}

impl AnthropicClassifier {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            endpoint: ENDPOINT.into(),
        }
    }

    /// Resolve adapter config from optional host-supplied values.
    /// `api_key`: `None` falls back to the [`API_KEY_ENV_VAR`] env var; missing
    /// or whitespace-only → [`ConfigError::MissingApiKey`].
    /// `model`: `None` defaults to [`DEFAULT_MODEL`].
    /// Encapsulates Anthropic-specific knowledge so the composition root just
    /// passes through the host's optional config fields.
    pub fn from_config(
        api_key: Option<String>,
        model: Option<String>,
    ) -> Result<Self, ConfigError> {
        let api_key = api_key
            .or_else(|| std::env::var(API_KEY_ENV_VAR).ok())
            .filter(|k| !k.trim().is_empty())
            .ok_or(ConfigError::MissingApiKey)?;
        let model = model
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.into());
        Ok(Self::new(api_key, model))
    }

    /// Override the endpoint — used by tests against a local mock server.
    #[doc(hidden)]
    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = endpoint;
        self
    }
}

impl LlmClassifier for AnthropicClassifier {
    fn classify(
        &self,
        ctx: &ClassificationContext,
    ) -> Result<ClassifierOutcome, LlmClassifierError> {
        let started = std::time::Instant::now();
        let json = self.post(build_request(ctx, &self.model))?;

        let latency = started.elapsed();
        let (input_tokens, output_tokens) = parse_usage(&json);
        let parsed =
            parse_response(&json).map_err(|e| LlmClassifierError::Unavailable(e.into()))?;

        let stats = LlmCallStats {
            model: self.model.clone(),
            latency,
            input_tokens,
            output_tokens,
        };

        Ok(match parsed {
            ParsedDecision::Green => ClassifierOutcome {
                verdict: Verdict::Green,
                raw_verdict: RawVerdict::Green,
                raw_reason: None,
                llm: Some(stats),
            },
            ParsedDecision::Red { reason } => ClassifierOutcome {
                verdict: Verdict::Red {
                    kind: RedKind::OutOfScope,
                    reason: reason.clone(),
                },
                raw_verdict: RawVerdict::Red,
                raw_reason: Some(reason),
                llm: Some(stats),
            },
        })
    }

    fn extract_scope(&self, text: &str) -> Result<Option<Scope>, LlmClassifierError> {
        if text.len() > MAX_EXTRACTION_TEXT_BYTES {
            return Err(LlmClassifierError::Unavailable(
                ExtractionError::TextTooLarge.into(),
            ));
        }
        if carries_reserved_tag(text) {
            return Err(LlmClassifierError::Unavailable(
                ExtractionError::ReservedTag.into(),
            ));
        }
        let json = self.post(build_extraction_request(text, &self.model))?;
        parse_extraction_response(&json).map_err(|e| LlmClassifierError::Unavailable(e.into()))
    }
}

impl AnthropicClassifier {
    fn post(&self, body: Value) -> Result<Value, LlmClassifierError> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout_read(READ_TIMEOUT)
            .build();
        agent
            .post(&self.endpoint)
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .set("content-type", "application/json")
            .send_json(body)
            .map_err(|e| LlmClassifierError::Unavailable(Box::new(e)))?
            .into_json()
            .map_err(|e| LlmClassifierError::Unavailable(Box::new(e)))
    }
}

fn parse_usage(json: &Value) -> (Option<u32>, Option<u32>) {
    let usage = json.get("usage");
    let input = usage
        .and_then(|u| u.get("input_tokens"))
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok());
    let output = usage
        .and_then(|u| u.get("output_tokens"))
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok());
    (input, output)
}

fn build_request(ctx: &ClassificationContext, model: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "temperature": 0,
        "system": system_prompt(ctx.standing),
        "tools": [verdict_tool()],
        "tool_choice": {"type": "tool", "name": VERDICT_TOOL_NAME},
        "messages": [
            {
                "role": "user",
                "content": user_message(ctx.action, ctx.scope),
            }
        ],
    })
}

/// Schema-validated structured-output tool. The model is forced to call this
/// (`tool_choice`) so the response shape can't drift across model revs.
fn verdict_tool() -> Value {
    json!({
        "name": VERDICT_TOOL_NAME,
        "description": "Emit the firewall verdict for the given scope and action.",
        "input_schema": {
            "type": "object",
            "properties": {
                "verdict": {
                    "type": "string",
                    "enum": ["green", "red"],
                    "description": "green = action plausibly serves the scope; red = it does not.",
                },
                "reason": {
                    "type": "string",
                    "description": "Required when verdict is red. One short sentence.",
                },
            },
            "required": ["verdict"],
        },
    })
}

fn user_message(action: &Action, scope: &Scope) -> String {
    let args_str: &str = action.args.as_ref();
    let truncated_args = truncate_at_char_boundary(args_str, MAX_ARGS_BYTES);
    let truncation_marker = if truncated_args.len() < args_str.len() {
        "\n…[truncated]"
    } else {
        ""
    };
    format!(
        "<scope>{}</scope>\n<action_tool>{}</action_tool>\n<action_input>{}{}</action_input>",
        scope.as_ref(),
        action.kind,
        truncated_args,
        truncation_marker,
    )
}

/// Truncate to ≤ `max_bytes` without splitting a UTF-8 codepoint.
fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[derive(Deserialize)]
struct Decision {
    verdict: String,
    #[serde(default)]
    reason: Option<String>,
}

enum ParsedDecision {
    Green,
    Red { reason: String },
}

fn parse_response(json: &Value) -> Result<ParsedDecision, ParseError> {
    let input = json
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| {
            blocks.iter().find(|b| {
                b.get("type").and_then(Value::as_str) == Some("tool_use")
                    && b.get("name").and_then(Value::as_str) == Some(VERDICT_TOOL_NAME)
            })
        })
        .and_then(|b| b.get("input"))
        .ok_or(ParseError::MissingVerdictToolUse)?;

    let decision: Decision = serde_json::from_value(input.clone())
        .map_err(|e| ParseError::InvalidToolInput(e.to_string()))?;

    match decision.verdict.as_str() {
        "green" => Ok(ParsedDecision::Green),
        "red" => {
            let reason = decision
                .reason
                .filter(|r| !r.trim().is_empty())
                .ok_or(ParseError::RedWithoutReason)?;
            Ok(ParsedDecision::Red { reason })
        }
        other => Err(ParseError::UnknownVerdict(other.into())),
    }
}

#[derive(Debug, thiserror::Error)]
enum ParseError {
    #[error("response had no `verdict` tool_use block")]
    MissingVerdictToolUse,
    #[error("`verdict` tool input did not match schema: {0}")]
    InvalidToolInput(String),
    #[error("verdict was \"red\" but reason was missing or empty")]
    RedWithoutReason,
    #[error("unknown verdict: {0:?}")]
    UnknownVerdict(String),
}

fn build_extraction_request(text: &str, model: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": EXTRACTION_MAX_TOKENS,
        "temperature": 0,
        "system": format!("{SCOPE_EXTRACTION_GUIDANCE}\n\n{EXTRACTION_WIRE_DIRECTIVES}"),
        "tools": [extract_scope_tool()],
        "tool_choice": {"type": "tool", "name": EXTRACT_SCOPE_TOOL_NAME},
        "messages": [
            {
                "role": "user",
                "content": format!("<text>{text}</text>"),
            }
        ],
    })
}

fn extract_scope_tool() -> Value {
    json!({
        "name": EXTRACT_SCOPE_TOOL_NAME,
        "description": "Report the task extracted from the text, or that it holds none.",
        "input_schema": {
            "type": "object",
            "properties": {
                "has_task": {
                    "type": "boolean",
                    "description": "true = the text asks for something concrete; false = it does not.",
                },
                "scope": {
                    "type": "string",
                    "description": "Required when has_task is true. The extracted task.",
                },
            },
            "required": ["has_task"],
        },
    })
}

fn parse_extraction_response(json: &Value) -> Result<Option<Scope>, ExtractionError> {
    if json.get("stop_reason").and_then(Value::as_str) == Some("max_tokens") {
        return Err(ExtractionError::Truncated);
    }
    let input = json
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| {
            blocks.iter().find(|b| {
                b.get("type").and_then(Value::as_str) == Some("tool_use")
                    && b.get("name").and_then(Value::as_str) == Some(EXTRACT_SCOPE_TOOL_NAME)
            })
        })
        .and_then(|b| b.get("input"))
        .ok_or(ExtractionError::MissingToolUse)?;

    let has_task = input
        .get("has_task")
        .and_then(Value::as_bool)
        .ok_or(ExtractionError::InvalidToolInput)?;
    if !has_task {
        return Ok(None);
    }
    let extracted = input
        .get("scope")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(ExtractionError::EmptyScope)?;
    if carries_reserved_tag(extracted) {
        return Err(ExtractionError::ReservedTag);
    }
    Scope::try_new(extracted)
        .map(Some)
        .map_err(|_| ExtractionError::InvalidScope)
}

/// Why an extraction produced no usable answer. Fixed messages only: the text
/// and the model's output are potentially sensitive and these errors reach
/// the audit log, so no variant carries content.
#[derive(Debug, thiserror::Error)]
enum ExtractionError {
    #[error("text exceeds the extraction size limit")]
    TextTooLarge,
    #[error("extraction response was cut off by max_tokens")]
    Truncated,
    #[error("response had no `extract_scope` tool_use block")]
    MissingToolUse,
    #[error("`extract_scope` tool input did not match schema")]
    InvalidToolInput,
    #[error("has_task was true but scope was missing or empty")]
    EmptyScope,
    #[error("text or extracted scope contained a reserved tag")]
    ReservedTag,
    #[error("extracted scope failed validation")]
    InvalidScope,
}
