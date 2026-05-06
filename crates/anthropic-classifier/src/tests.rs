use domain::{Action, ActionArgs, ActionKind, Scope};
use serde_json::json;

use super::*;

fn action(kind: &str, args: &str) -> Action {
    Action {
        call_id: None,
        kind: ActionKind::new(kind),
        args: ActionArgs::new(args),
    }
}

fn scope(s: &str) -> Scope {
    Scope::try_new(s).unwrap()
}

fn green_response() -> serde_json::Value {
    json!({
        "content": [
            {"type": "tool_use", "id": "toolu_1", "name": "verdict", "input": {"verdict": "green"}}
        ]
    })
}

#[test]
fn request_includes_model_max_tokens_and_system_prompt() {
    let body = build_request(
        &action("Bash", "ls"),
        &scope("do the thing"),
        "claude-haiku-4-5",
    );
    assert_eq!(body["model"], "claude-haiku-4-5");
    assert_eq!(body["max_tokens"], MAX_TOKENS);
    let system = body["system"].as_str().unwrap();
    assert!(
        system.contains(CLASSIFIER_GUIDANCE),
        "system prompt must include the domain's CLASSIFIER_GUIDANCE verbatim"
    );
    assert!(
        system.contains("verdict"),
        "system prompt must include wire directive about the verdict tool"
    );
}

#[test]
fn request_forces_verdict_tool_use() {
    let body = build_request(
        &action("Bash", "ls"),
        &scope("anything"),
        "claude-haiku-4-5",
    );
    assert_eq!(body["tool_choice"]["type"], "tool");
    assert_eq!(body["tool_choice"]["name"], VERDICT_TOOL_NAME);
    let tools = body["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], VERDICT_TOOL_NAME);
    let schema = &tools[0]["input_schema"];
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["required"], json!(["verdict"]));
    let verdict_enum = schema["properties"]["verdict"]["enum"].as_array().unwrap();
    assert_eq!(verdict_enum, &[json!("green"), json!("red")]);
}

#[test]
fn request_user_message_wraps_scope_and_action_in_tags() {
    let body = build_request(
        &action("Bash", "cat /etc/passwd"),
        &scope("front-end work"),
        "claude-haiku-4-5",
    );
    let user_text = body["messages"][0]["content"].as_str().unwrap();
    assert!(user_text.contains("<scope>front-end work</scope>"));
    assert!(user_text.contains("<action_tool>Bash</action_tool>"));
    assert!(user_text.contains("<action_input>cat /etc/passwd</action_input>"));
    assert_eq!(body["messages"][0]["role"], "user");
}

#[test]
fn user_message_truncates_oversized_args() {
    let big = "x".repeat(MAX_ARGS_BYTES * 2);
    let body = build_request(
        &action("Bash", &big),
        &scope("anything"),
        "claude-haiku-4-5",
    );
    let user_text = body["messages"][0]["content"].as_str().unwrap();
    assert!(user_text.contains("…[truncated]"));
    assert!(user_text.len() < big.len());
}

#[test]
fn truncate_respects_utf8_char_boundary() {
    let s = "𓂀".repeat(MAX_ARGS_BYTES);
    let truncated = truncate_at_char_boundary(&s, MAX_ARGS_BYTES);
    assert!(truncated.is_char_boundary(truncated.len()));
    assert!(truncated.len() <= MAX_ARGS_BYTES);
}

mod from_config {
    use super::*;
    use std::sync::Mutex;

    // Env var manipulation must be serialised across tests in this module to avoid races.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_clean_env<R>(f: impl FnOnce() -> R) -> R {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: serialised by ENV_LOCK; restored to whatever was there before.
        let prev = std::env::var(API_KEY_ENV_VAR).ok();
        unsafe { std::env::remove_var(API_KEY_ENV_VAR) };
        let result = f();
        unsafe {
            match prev {
                Some(v) => std::env::set_var(API_KEY_ENV_VAR, v),
                None => std::env::remove_var(API_KEY_ENV_VAR),
            }
        }
        result
    }

    #[test]
    fn explicit_key_takes_precedence_over_env() {
        with_clean_env(|| {
            unsafe { std::env::set_var(API_KEY_ENV_VAR, "from-env") };
            let c = AnthropicClassifier::from_config(Some("from-arg".into()), None).unwrap();
            assert_eq!(c.api_key, "from-arg");
        });
    }

    #[test]
    fn falls_back_to_env_when_arg_is_none() {
        with_clean_env(|| {
            unsafe { std::env::set_var(API_KEY_ENV_VAR, "from-env") };
            let c = AnthropicClassifier::from_config(None, None).unwrap();
            assert_eq!(c.api_key, "from-env");
        });
    }

    #[test]
    fn missing_when_neither_arg_nor_env_is_set() {
        with_clean_env(|| {
            assert!(matches!(
                AnthropicClassifier::from_config(None, None),
                Err(ConfigError::MissingApiKey)
            ));
        });
    }

    #[test]
    fn whitespace_arg_treated_as_missing_when_no_env() {
        with_clean_env(|| {
            assert!(matches!(
                AnthropicClassifier::from_config(Some("   ".into()), None),
                Err(ConfigError::MissingApiKey)
            ));
        });
    }

    #[test]
    fn defaults_model_to_default_model_const() {
        with_clean_env(|| {
            let c = AnthropicClassifier::from_config(Some("k".into()), None).unwrap();
            assert_eq!(c.model, DEFAULT_MODEL);
        });
    }

    #[test]
    fn explicit_model_overrides_default() {
        with_clean_env(|| {
            let c = AnthropicClassifier::from_config(Some("k".into()), Some("custom-model".into()))
                .unwrap();
            assert_eq!(c.model, "custom-model");
        });
    }

    #[test]
    fn whitespace_model_falls_back_to_default() {
        with_clean_env(|| {
            let c = AnthropicClassifier::from_config(Some("k".into()), Some("  ".into())).unwrap();
            assert_eq!(c.model, DEFAULT_MODEL);
        });
    }
}

#[test]
fn parse_green_verdict_tool_use() {
    assert!(matches!(
        parse_response(&green_response()),
        Ok(LlmDecision::Green)
    ));
}

#[test]
fn parse_red_verdict_with_reason() {
    let json = json!({
        "content": [
            {
                "type": "tool_use",
                "id": "toolu_2",
                "name": "verdict",
                "input": {"verdict": "red", "reason": "reads SSH key"}
            }
        ]
    });
    let LlmDecision::Red { reason } = parse_response(&json).unwrap() else {
        panic!("expected Red");
    };
    assert_eq!(reason, "reads SSH key");
}

#[test]
fn parse_skips_non_tool_use_blocks() {
    let json = json!({
        "content": [
            {"type": "text", "text": "thinking aloud..."},
            {"type": "tool_use", "id": "toolu_3", "name": "verdict", "input": {"verdict": "green"}}
        ]
    });
    assert!(matches!(parse_response(&json), Ok(LlmDecision::Green)));
}

#[test]
fn parse_ignores_other_tool_use_blocks() {
    let json = json!({
        "content": [
            {"type": "tool_use", "id": "toolu_a", "name": "something_else", "input": {"foo": "bar"}}
        ]
    });
    assert!(matches!(
        parse_response(&json),
        Err(ParseError::MissingVerdictToolUse)
    ));
}

#[test]
fn parse_fails_when_no_tool_use_block() {
    let json = json!({"content": []});
    assert!(matches!(
        parse_response(&json),
        Err(ParseError::MissingVerdictToolUse)
    ));
}

#[test]
fn parse_fails_on_unknown_verdict() {
    let json = json!({
        "content": [
            {"type": "tool_use", "id": "x", "name": "verdict", "input": {"verdict": "yellow"}}
        ]
    });
    assert!(matches!(
        parse_response(&json),
        Err(ParseError::UnknownVerdict(_))
    ));
}

#[test]
fn parse_fails_on_red_without_reason() {
    let json = json!({
        "content": [
            {"type": "tool_use", "id": "x", "name": "verdict", "input": {"verdict": "red"}}
        ]
    });
    assert!(matches!(
        parse_response(&json),
        Err(ParseError::RedWithoutReason)
    ));
}

#[test]
fn parse_fails_on_red_with_empty_reason() {
    let json = json!({
        "content": [
            {"type": "tool_use", "id": "x", "name": "verdict", "input": {"verdict": "red", "reason": "   "}}
        ]
    });
    assert!(matches!(
        parse_response(&json),
        Err(ParseError::RedWithoutReason)
    ));
}

#[test]
fn parse_fails_when_input_shape_wrong() {
    let json = json!({
        "content": [
            {"type": "tool_use", "id": "x", "name": "verdict", "input": {"verdict": 42}}
        ]
    });
    assert!(matches!(
        parse_response(&json),
        Err(ParseError::InvalidToolInput(_))
    ));
}
