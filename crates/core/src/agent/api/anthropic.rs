//! The Anthropic Messages API (`POST {base}/v1/messages`).
//!
//! Non-streaming: one request per `complete`. Wire shape (from the public
//! Messages docs): headers `x-api-key`, `anthropic-version: 2023-06-01`;
//! request `{model, max_tokens, system, messages, tools:[{name, description,
//! input_schema}]}` where message content is a list of blocks (`text`,
//! `tool_use`, `tool_result`); reply `{content:[blocks], stop_reason,
//! usage:{input_tokens, output_tokens}}`; errors
//! `{"type":"error","error":{"type","message"}}`. Roles must alternate, so
//! consecutive tool results are merged into one user message.

use std::fmt;
use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::openai::{
    CHECK_TIMEOUT, REQUEST_TIMEOUT, TransportError, http_call, scrub,
};
use super::{BackendError, ChatBackend, Completion, Message, StopReason, ToolCall, ToolSpec};
use crate::agent::types::Usage;

pub const DEFAULT_BASE: &str = "https://api.anthropic.com";
pub const BASE_ENV: &str = "INFINABOX_ANTHROPIC_BASE";

const API_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u64 = 8192;

pub struct AnthropicMessages {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

// Hand-written so the key can never reach a log through `{:?}`.
impl fmt::Debug for AnthropicMessages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnthropicMessages")
            .field("base_url", &self.base_url)
            .field("api_key", &"<hidden>")
            .field("model", &self.model)
            .finish()
    }
}

impl AnthropicMessages {
    fn base(&self) -> &str {
        self.base_url.trim_end_matches('/')
    }

    fn headers(&self) -> Vec<(String, String)> {
        vec![
            ("x-api-key".into(), self.api_key.clone()),
            ("anthropic-version".into(), API_VERSION.into()),
            ("content-type".into(), "application/json".into()),
        ]
    }

    fn build_body(&self, system: &str, messages: &[Message], tools: &[ToolSpec]) -> Value {
        let mut msgs: Vec<Value> = Vec::new();
        // Appends blocks to the last message when it is already a user turn
        // (the API requires alternating roles), else starts a new one.
        fn push_user(msgs: &mut Vec<Value>, block: Value) {
            if let Some(last) = msgs.last_mut() {
                if last["role"] == "user" {
                    last["content"].as_array_mut().unwrap().push(block);
                    return;
                }
            }
            msgs.push(json!({"role": "user", "content": [block]}));
        }
        for m in messages {
            match m {
                Message::User(text) => push_user(&mut msgs, json!({"type": "text", "text": text})),
                Message::Assistant { text, tool_calls } => {
                    let mut blocks = Vec::new();
                    if let Some(t) = text.as_deref().filter(|t| !t.is_empty()) {
                        blocks.push(json!({"type": "text", "text": t}));
                    }
                    for c in tool_calls {
                        blocks.push(json!({
                            "type": "tool_use", "id": c.id, "name": c.name, "input": c.arguments,
                        }));
                    }
                    msgs.push(json!({"role": "assistant", "content": blocks}));
                }
                Message::ToolResult { call_id, content, is_error } => push_user(
                    &mut msgs,
                    json!({
                        "type": "tool_result", "tool_use_id": call_id,
                        "content": content, "is_error": is_error,
                    }),
                ),
            }
        }
        let mut body = json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "system": system,
            "messages": msgs,
        });
        if !tools.is_empty() {
            body["tools"] = tools
                .iter()
                .map(|t| json!({
                    "name": t.name, "description": t.description, "input_schema": t.parameters,
                }))
                .collect();
        }
        body
    }

    fn classify_error(&self, status: u16, body: &str) -> BackendError {
        let parsed: Option<Value> = serde_json::from_str(body).ok();
        let err = parsed.as_ref().and_then(|v| v.get("error"));
        let kind = err.and_then(|e| e.get("type")).and_then(Value::as_str).unwrap_or("");
        let message = err
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty())
            .map(|m| scrub(m, Some(&self.api_key)));
        match status {
            401 | 403 => BackendError::NotAuthenticated(
                "Anthropic didn't accept the API key. Check it and try again.".into(),
            ),
            429 | 529 => BackendError::RateLimited(
                message.unwrap_or_else(|| "Anthropic is busy or a limit was reached.".into()),
            ),
            _ if kind == "overloaded_error" || kind == "rate_limit_error" => {
                BackendError::RateLimited(
                    message.unwrap_or_else(|| "Anthropic is busy or a limit was reached.".into()),
                )
            }
            _ => BackendError::Other(message.unwrap_or_else(|| {
                format!("Anthropic answered with an error (HTTP {status}).")
            })),
        }
    }
}

fn parse_completion(body: &str) -> Result<Completion, BackendError> {
    let bad = || BackendError::Other("Anthropic sent a reply InfinaBox couldn't read.".to_string());
    let v: Value = serde_json::from_str(body).map_err(|_| bad())?;
    let blocks = v.get("content").and_then(Value::as_array).ok_or_else(bad)?;

    let mut text = String::new();
    let mut tool_calls = Vec::new();
    for b in blocks {
        match b.get("type").and_then(Value::as_str) {
            Some("text") => text.push_str(b.get("text").and_then(Value::as_str).unwrap_or("")),
            Some("tool_use") => {
                let (Some(id), Some(name)) =
                    (b.get("id").and_then(Value::as_str), b.get("name").and_then(Value::as_str))
                else {
                    continue;
                };
                tool_calls.push(ToolCall {
                    id: id.to_string(),
                    name: name.to_string(),
                    arguments: b.get("input").cloned().unwrap_or_else(|| json!({})),
                });
            }
            _ => {}
        }
    }

    let stop = match v.get("stop_reason").and_then(Value::as_str) {
        Some("end_turn" | "stop_sequence") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        // "refusal", "pause_turn", anything new.
        _ => StopReason::Other,
    };
    let usage = v.get("usage").filter(|u| u.is_object()).map(|u| Usage {
        input_tokens: u.get("input_tokens").and_then(Value::as_u64),
        output_tokens: u.get("output_tokens").and_then(Value::as_u64),
    });

    Ok(Completion {
        text: Some(text).filter(|t| !t.is_empty()),
        tool_calls,
        usage,
        stop,
    })
}

impl ChatBackend for AnthropicMessages {
    fn label(&self) -> String {
        "Anthropic API".into()
    }
    fn model(&self) -> String {
        self.model.clone()
    }
    fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolSpec],
        cancel: &AtomicBool,
    ) -> Result<Completion, BackendError> {
        let body = self.build_body(system, messages, tools).to_string();
        let url = format!("{}/v1/messages", self.base());
        match http_call("POST", url, self.headers(), Some(body), REQUEST_TIMEOUT, cancel) {
            Ok(r) if (200..300).contains(&r.status) => parse_completion(&r.body),
            Ok(r) => Err(self.classify_error(r.status, &r.body)),
            Err(TransportError::Cancelled) => Err(BackendError::Cancelled),
            Err(TransportError::Unreachable) => Err(BackendError::Unreachable(format!(
                "Couldn't reach Anthropic at {}. Check your internet connection.",
                self.base()
            ))),
            Err(TransportError::Other(m)) => Err(BackendError::Other(m)),
        }
    }

    fn check(&self) -> Option<bool> {
        let url = format!("{}/v1/models", self.base());
        let never = AtomicBool::new(false);
        match http_call("GET", url, self.headers(), None, CHECK_TIMEOUT, &never) {
            Ok(r) if r.status == 200 => Some(true),
            Ok(r) if r.status == 401 || r.status == 403 => Some(false),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::openai::fake::{Reply, dead_base, reply, serve};
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    const KEY: &str = "test-key-123";

    fn backend(base: &str) -> AnthropicMessages {
        AnthropicMessages {
            base_url: base.to_string(),
            api_key: KEY.into(),
            model: "test-model".into(),
        }
    }

    fn tool() -> ToolSpec {
        ToolSpec {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
        }
    }

    fn run(
        b: &AnthropicMessages,
        msgs: &[Message],
        tools: &[ToolSpec],
    ) -> Result<Completion, BackendError> {
        b.complete("You are helpful.", msgs, tools, &AtomicBool::new(false))
    }

    #[test]
    fn sends_exact_request_merges_tool_results_and_reads_text() {
        let (base, seen) = serve(vec![reply(
            200,
            r#"{"id":"msg_1","type":"message","role":"assistant",
                "content":[{"type":"text","text":"Hello"},{"type":"text","text":" there"}],
                "stop_reason":"end_turn","usage":{"input_tokens":12,"output_tokens":4}}"#,
        )]);
        let call = |id: &str| ToolCall {
            id: id.into(),
            name: "read_file".into(),
            arguments: json!({"path": "a.gd"}),
        };
        let msgs = vec![
            Message::User("hi".into()),
            Message::Assistant { text: Some("Looking.".into()), tool_calls: vec![call("t1"), call("t2")] },
            Message::ToolResult { call_id: "t1".into(), content: "one".into(), is_error: false },
            Message::ToolResult { call_id: "t2".into(), content: "boom".into(), is_error: true },
        ];
        let done = run(&backend(&base), &msgs, &[tool()]).unwrap();
        assert_eq!(done.text.as_deref(), Some("Hello there"));
        assert_eq!(done.stop, StopReason::EndTurn);
        let u = done.usage.unwrap();
        assert_eq!((u.input_tokens, u.output_tokens), (Some(12), Some(4)));

        let seen = seen.lock().unwrap();
        let s = &seen[0];
        assert_eq!(s.method, "POST");
        assert_eq!(s.path, "/v1/messages");
        assert_eq!(s.header("x-api-key"), Some("test-key-123"));
        assert_eq!(s.header("anthropic-version"), Some("2023-06-01"));
        assert_eq!(s.header("content-type"), Some("application/json"));
        assert_eq!(s.header("user-agent"), Some("InfinaBox"));
        let body: Value = serde_json::from_str(&s.body).unwrap();
        assert_eq!(
            body,
            json!({
                "model": "test-model",
                "max_tokens": 8192,
                "system": "You are helpful.",
                "messages": [
                    {"role":"user","content":[{"type":"text","text":"hi"}]},
                    {"role":"assistant","content":[
                        {"type":"text","text":"Looking."},
                        {"type":"tool_use","id":"t1","name":"read_file","input":{"path":"a.gd"}},
                        {"type":"tool_use","id":"t2","name":"read_file","input":{"path":"a.gd"}}
                    ]},
                    {"role":"user","content":[
                        {"type":"tool_result","tool_use_id":"t1","content":"one","is_error":false},
                        {"type":"tool_result","tool_use_id":"t2","content":"boom","is_error":true}
                    ]}
                ],
                "tools": [{
                    "name":"read_file","description":"Read a file",
                    "input_schema": {"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}
                }]
            })
        );
    }

    #[test]
    fn no_tools_field_when_empty() {
        let (base, seen) = serve(vec![reply(
            200,
            r#"{"content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn"}"#,
        )]);
        let done = run(&backend(&base), &[Message::User("x".into())], &[]).unwrap();
        assert_eq!(done.text.as_deref(), Some("ok"));
        assert!(done.usage.is_none());
        let body: Value = serde_json::from_str(&seen.lock().unwrap()[0].body).unwrap();
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn tool_use_response() {
        let (base, _) = serve(vec![reply(
            200,
            r#"{"content":[{"type":"text","text":"Let me look."},
                {"type":"tool_use","id":"toolu_1","name":"read_file","input":{"path":"x.gd"}}],
                "stop_reason":"tool_use","usage":{"input_tokens":1,"output_tokens":2}}"#,
        )]);
        let done = run(&backend(&base), &[Message::User("x".into())], &[tool()]).unwrap();
        assert_eq!(done.stop, StopReason::ToolUse);
        assert_eq!(done.text.as_deref(), Some("Let me look."));
        assert_eq!(
            done.tool_calls,
            vec![ToolCall { id: "toolu_1".into(), name: "read_file".into(), arguments: json!({"path":"x.gd"}) }]
        );
    }

    #[test]
    fn stop_reasons() {
        let mk = |r: &str| reply(200, &format!(r#"{{"content":[{{"type":"text","text":"a"}}],"stop_reason":"{r}"}}"#));
        let (base, _) = serve(vec![mk("max_tokens"), mk("stop_sequence"), mk("refusal")]);
        let b = backend(&base);
        let m = [Message::User("x".into())];
        assert_eq!(run(&b, &m, &[]).unwrap().stop, StopReason::MaxTokens);
        assert_eq!(run(&b, &m, &[]).unwrap().stop, StopReason::EndTurn);
        assert_eq!(run(&b, &m, &[]).unwrap().stop, StopReason::Other);
    }

    #[test]
    fn error_statuses() {
        let err = |t: &str, m: &str| format!(r#"{{"type":"error","error":{{"type":"{t}","message":"{m}"}}}}"#);
        let (base, _) = serve(vec![
            reply(401, &err("authentication_error", "invalid x-api-key test-key-123")),
            reply(403, &err("permission_error", "no")),
            reply(429, &err("rate_limit_error", "Number of requests has exceeded your limit")),
            reply(529, &err("overloaded_error", "Overloaded")),
            reply(500,&err("overloaded_error", "Overloaded")),
            reply(400, &err("invalid_request_error", "max_tokens too big")),
            reply(500, "not json"),
        ]);
        let b = backend(&base);
        let m = [Message::User("x".into())];
        let e401 = run(&b, &m, &[]).unwrap_err();
        assert!(matches!(e401, BackendError::NotAuthenticated(_)));
        assert!(matches!(run(&b, &m, &[]).unwrap_err(), BackendError::NotAuthenticated(_)));
        match run(&b, &m, &[]).unwrap_err() {
            BackendError::RateLimited(msg) => assert!(msg.contains("exceeded your limit")),
            e => panic!("{e:?}"),
        }
        assert!(matches!(run(&b, &m, &[]).unwrap_err(), BackendError::RateLimited(_)));
        assert!(matches!(run(&b, &m, &[]).unwrap_err(), BackendError::RateLimited(_)));
        assert_eq!(
            run(&b, &m, &[]).unwrap_err(),
            BackendError::Other("max_tokens too big".into())
        );
        match run(&b, &m, &[]).unwrap_err() {
            BackendError::Other(msg) => assert!(msg.contains("500")),
            e => panic!("{e:?}"),
        }
        assert!(!format!("{e401:?}").contains(KEY));
    }

    #[test]
    fn echoed_key_is_scrubbed_from_messages() {
        let (base, _) = serve(vec![reply(
            400,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad test-key-123"}}"#,
        )]);
        let e = run(&backend(&base), &[Message::User("x".into())], &[]).unwrap_err();
        assert_eq!(e, BackendError::Other("bad …".into()));
    }

    #[test]
    fn connection_refused_is_unreachable() {
        let e = run(&backend(&dead_base()), &[Message::User("x".into())], &[]).unwrap_err();
        assert!(matches!(e, BackendError::Unreachable(_)), "{e:?}");
        assert!(!format!("{e:?}").contains(KEY));
    }

    #[test]
    fn cancel_while_server_sleeps() {
        let (base, _) = serve(vec![Reply {
            status: 200,
            body: "{}".into(),
            delay: Duration::from_secs(5),
        }]);
        let b = backend(&base);
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            c2.store(true, Ordering::SeqCst);
        });
        let t = Instant::now();
        let e = b.complete("s", &[Message::User("x".into())], &[], &cancel).unwrap_err();
        assert_eq!(e, BackendError::Cancelled);
        assert!(t.elapsed() < Duration::from_secs(2), "took {:?}", t.elapsed());
    }

    #[test]
    fn check_maps_statuses() {
        let (base, seen) =
            serve(vec![reply(200, "{}"), reply(401, "{}"), reply(403, "{}"), reply(500, "{}")]);
        let b = backend(&base);
        assert_eq!(b.check(), Some(true));
        assert_eq!(b.check(), Some(false));
        assert_eq!(b.check(), Some(false));
        assert_eq!(b.check(), None);
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].method, "GET");
        assert_eq!(seen[0].path, "/v1/models");
        assert_eq!(seen[0].header("x-api-key"), Some("test-key-123"));
        assert_eq!(seen[0].header("anthropic-version"), Some("2023-06-01"));
        assert_eq!(backend(&dead_base()).check(), None);
    }

    #[test]
    fn debug_hides_the_key() {
        let d = format!("{:?}", backend("http://x"));
        assert!(!d.contains(KEY));
        assert!(d.contains("<hidden>"));
    }

    /// Talks to the real Messages API. The key comes from the environment
    /// only and is never printed.
    #[test]
    #[ignore = "needs ANTHROPIC_API_KEY; run with --ignored"]
    fn real_api_round_trip() {
        let Ok(key) = std::env::var("ANTHROPIC_API_KEY") else {
            eprintln!("ANTHROPIC_API_KEY is not set: the real Anthropic API was NOT exercised");
            return;
        };
        let b = AnthropicMessages {
            base_url: DEFAULT_BASE.into(),
            api_key: key,
            model: "claude-haiku-4-5-20251001".into(),
        };
        assert_eq!(b.check(), Some(true));
        let done = b
            .complete(
                "Answer in one short sentence.",
                &[Message::User("Say hello.".into())],
                &[],
                &AtomicBool::new(false),
            )
            .expect("real completion");
        assert!(!done.text.unwrap_or_default().trim().is_empty());
        assert!(done.usage.is_some());
    }
}
