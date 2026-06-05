use std::sync::{Arc, Mutex};
use std::thread;

use anthropic_classifier::AnthropicClassifier;
use domain::{
    Action, ActionArgs, ActionKind, ClassificationContext, LlmClassifier, RawVerdict, Scope,
    StandingGuidance, Verdict,
};
use tiny_http::{Header, Response, Server};

struct CapturedRequest {
    body: String,
    api_key: Option<String>,
    anthropic_version: Option<String>,
    method: String,
}

struct MockServer {
    url: String,
    captured: Arc<Mutex<Option<CapturedRequest>>>,
    _thread: thread::JoinHandle<()>,
}

fn spawn_mock(response_body: &'static str, status: u16) -> MockServer {
    let server = Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/messages", server.server_addr());
    let captured = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);
    let handle = thread::spawn(move || {
        if let Ok(mut req) = server.recv() {
            let api_key = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("x-api-key"))
                .map(|h| h.value.as_str().to_string());
            let anthropic_version = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("anthropic-version"))
                .map(|h| h.value.as_str().to_string());
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            *captured_clone.lock().unwrap() = Some(CapturedRequest {
                body,
                api_key,
                anthropic_version,
                method: format!("{:?}", req.method()),
            });
            let response = Response::from_string(response_body)
                .with_status_code(status)
                .with_header("content-type: application/json".parse::<Header>().unwrap());
            let _ = req.respond(response);
        }
    });
    MockServer {
        url,
        captured,
        _thread: handle,
    }
}

fn action() -> Action {
    Action {
        call_id: None,
        kind: ActionKind::new("Bash"),
        args: ActionArgs::new("ls"),
    }
}

fn scope() -> Scope {
    Scope::try_new("do the thing").unwrap()
}

fn ctx<'a>(
    action: &'a Action,
    scope: &'a Scope,
    standing: &'a StandingGuidance,
) -> ClassificationContext<'a> {
    ClassificationContext {
        action,
        scope,
        standing,
    }
}

#[test]
fn classify_sends_post_with_required_headers_and_forces_verdict_tool() {
    let mock = spawn_mock(
        r#"{"content":[{"type":"tool_use","id":"toolu_1","name":"verdict","input":{"verdict":"green"}}]}"#,
        200,
    );
    let classifier = AnthropicClassifier::new("sk-test".into(), "claude-haiku-4-5".into())
        .with_endpoint(mock.url.clone());
    let outcome = classifier
        .classify(&ctx(&action(), &scope(), &StandingGuidance::default()))
        .unwrap();
    assert!(matches!(outcome.verdict, Verdict::Green));
    assert_eq!(outcome.raw_verdict, RawVerdict::Green);
    let stats = outcome.llm.expect("LlmCallStats populated");
    assert_eq!(stats.model, "claude-haiku-4-5");
    assert!(stats.latency >= std::time::Duration::ZERO);

    let captured = mock.captured.lock().unwrap();
    let req = captured.as_ref().expect("request was captured");
    assert_eq!(req.method, "Post");
    assert_eq!(req.api_key.as_deref(), Some("sk-test"));
    assert_eq!(req.anthropic_version.as_deref(), Some("2023-06-01"));
    let body: serde_json::Value = serde_json::from_str(&req.body).unwrap();
    assert_eq!(body["model"], "claude-haiku-4-5");
    assert_eq!(body["tool_choice"]["type"], "tool");
    assert_eq!(body["tool_choice"]["name"], "verdict");
    assert_eq!(body["tools"][0]["name"], "verdict");
}

#[test]
fn classify_returns_red_when_server_says_red() {
    let mock = spawn_mock(
        r#"{"content":[{"type":"tool_use","id":"toolu_2","name":"verdict","input":{"verdict":"red","reason":"unrelated"}}]}"#,
        200,
    );
    let classifier = AnthropicClassifier::new("sk-test".into(), "claude-haiku-4-5".into())
        .with_endpoint(mock.url);
    let outcome = classifier
        .classify(&ctx(&action(), &scope(), &StandingGuidance::default()))
        .unwrap();
    let Verdict::Red { reason, .. } = &outcome.verdict else {
        panic!("expected Red");
    };
    assert_eq!(reason, "unrelated");
    assert_eq!(outcome.raw_verdict, RawVerdict::Red);
    assert_eq!(outcome.raw_reason.as_deref(), Some("unrelated"));
}

#[test]
fn classify_fails_unavailable_on_http_500() {
    let mock = spawn_mock(r#"{"error":"oops"}"#, 500);
    let classifier = AnthropicClassifier::new("sk-test".into(), "claude-haiku-4-5".into())
        .with_endpoint(mock.url);
    assert!(
        classifier
            .classify(&ctx(&action(), &scope(), &StandingGuidance::default()))
            .is_err()
    );
}

#[test]
fn classify_fails_unavailable_when_no_verdict_tool_use() {
    let mock = spawn_mock(
        r#"{"content":[{"type":"text","text":"I refuse to call the tool."}]}"#,
        200,
    );
    let classifier = AnthropicClassifier::new("sk-test".into(), "claude-haiku-4-5".into())
        .with_endpoint(mock.url);
    assert!(
        classifier
            .classify(&ctx(&action(), &scope(), &StandingGuidance::default()))
            .is_err()
    );
}
