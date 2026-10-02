//! OpenAI-compatible chat completions (`POST {base}/chat/completions`): the
//! OpenAI API itself, or a local model behind Ollama
//! (`http://localhost:11434/v1`) or LM Studio (`http://localhost:1234/v1`).
//!
//! Non-streaming: one request per `complete`. Wire shape (from the public
//! chat-completions docs): the request carries `model`, `messages` and
//! `tools:[{type:"function",function:{name,description,parameters}}]`; the
//! reply carries `choices[0].message.{content,tool_calls}`,
//! `choices[0].finish_reason` and `usage.{prompt,completion}_tokens`.
//!
//! This file also holds the small blocking-HTTP helper the Anthropic backend
//! shares (`http_call`): a worker thread does the request while the caller
//! polls the cancel flag.

use std::fmt;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

use super::{BackendError, ChatBackend, Completion, Message, StopReason, ToolCall, ToolSpec};
use crate::agent::types::Usage;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(300);
pub(super) const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const REQUEST_TIMEOUT: Duration = TOTAL_TIMEOUT;
const CANCEL_POLL: Duration = Duration::from_millis(100);
/// A reply larger than this is refused rather than buffered.
const MAX_RESPONSE_BYTES: u64 = 20 * 1024 * 1024;

pub struct OpenAiCompatible {
    /// Up to and including `/v1` (no trailing slash).
    pub base_url: String,
    /// `None` for local servers that need no key.
    pub api_key: Option<String>,
    pub model: String,
    /// "OpenAI API" or "Local model".
    pub label: String,
}

// Hand-written so the key can never reach a log through `{:?}`.
impl fmt::Debug for OpenAiCompatible {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiCompatible")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<hidden>"))
            .field("model", &self.model)
            .field("label", &self.label)
            .finish()
    }
}

// ---- shared blocking HTTP ---------------------------------------------------

pub(super) struct HttpResponse {
    pub status: u16,
    pub body: String,
}

pub(super) enum TransportError {
    Cancelled,
    /// Refused, DNS, timeout, dropped connection.
    Unreachable,
    /// Anything else (e.g. an oversized reply); a plain sentence.
    Other(String),
}

/// One HTTP request on a worker thread; the caller polls `cancel` every
/// ~100 ms. When cancelled the worker's eventual result is dropped.
pub(super) fn http_call(
    method: &'static str,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    total_timeout: Duration,
    cancel: &AtomicBool,
) -> Result<HttpResponse, TransportError> {
    if cancel.load(Ordering::SeqCst) {
        return Err(TransportError::Cancelled);
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(do_request(method, &url, &headers, body, total_timeout));
    });
    loop {
        match rx.recv_timeout(CANCEL_POLL) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancel.load(Ordering::SeqCst) {
                    return Err(TransportError::Cancelled);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(TransportError::Other("The request stopped unexpectedly.".into()));
            }
        }
    }
}

fn do_request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<String>,
    total_timeout: Duration,
) -> Result<HttpResponse, TransportError> {
    let mut builder = reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(total_timeout)
        .user_agent("InfinaBox");
    // A system proxy must never swallow a model running on this machine.
    if let Ok(parsed) = reqwest::Url::parse(url) {
        if matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]" | "::1")) {
            builder = builder.no_proxy();
        }
    }
    let client = builder
        .build()
        .map_err(|_| TransportError::Other("Couldn't set up the network request.".into()))?;
    let mut req = match method {
        "GET" => client.get(url),
        _ => client.post(url),
    };
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if let Some(b) = body {
        req = req.body(b);
    }
    // Error details are deliberately dropped: they can echo the URL/headers.
    let resp = req.send().map_err(|_| TransportError::Unreachable)?;
    let status = resp.status().as_u16();
    let mut bytes = Vec::new();
    resp.take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| TransportError::Unreachable)?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(TransportError::Other("The model's reply was too large to read.".into()));
    }
    Ok(HttpResponse { status, body: String::from_utf8_lossy(&bytes).into_owned() })
}

/// Removes the key from text that came from a server, in case it echoes it.
pub(super) fn scrub(text: &str, key: Option<&str>) -> String {
    match key {
        Some(k) if !k.is_empty() => text.replace(k, "…"),
        _ => text.to_string(),
    }
}

// ---- OpenAI wire format -----------------------------------------------------

impl OpenAiCompatible {
    fn base(&self) -> &str {
        self.base_url.trim_end_matches('/')
    }

    fn headers(&self) -> Vec<(String, String)> {
        let mut h = vec![("Content-Type".to_string(), "application/json".to_string())];
        if let Some(key) = self.api_key.as_deref().filter(|k| !k.is_empty()) {
            h.push(("Authorization".to_string(), format!("Bearer {key}")));
        }
        h
    }

    fn build_body(&self, system: &str, messages: &[Message], tools: &[ToolSpec]) -> Value {
        let mut msgs = vec![json!({"role": "system", "content": system})];
        for m in messages {
            msgs.push(match m {
                Message::User(text) => json!({"role": "user", "content": text}),
                Message::Assistant { text, tool_calls } => {
                    let mut o = json!({"role": "assistant", "content": text});
                    if !tool_calls.is_empty() {
                        o["tool_calls"] = tool_calls
                            .iter()
                            .map(|c| {
                                json!({
                                    "id": c.id,
                                    "type": "function",
                                    "function": {
                                        "name": c.name,
                                        // The API wants the arguments as a JSON *string*.
                                        "arguments": c.arguments.to_string(),
                                    },
                                })
                            })
                            .collect();
                    }
                    o
                }
                Message::ToolResult { call_id, content, .. } => {
                    json!({"role": "tool", "tool_call_id": call_id, "content": content})
                }
            });
        }
        let mut body = json!({"model": self.model, "messages": msgs});
        if !tools.is_empty() {
            body["tools"] = tools
                .iter()
                .map(|t| {
                    json!({"type": "function", "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }})
                })
                .collect();
            body["tool_choice"] = json!("auto");
        }
        body
    }

    fn unreachable(&self) -> BackendError {
        BackendError::Unreachable(format!(
            "Couldn't reach the model at {}. Is it running?",
            self.base()
        ))
    }

    fn classify_error(&self, status: u16, body: &str) -> BackendError {
        let key = self.api_key.as_deref();
        match status {
            401 | 403 => BackendError::NotAuthenticated(
                "The model server didn't accept the key. Check it and try again.".into(),
            ),
            429 => BackendError::RateLimited(
                "The model service is busy or a limit was reached. Wait a moment and try again."
                    .into(),
            ),
            _ => {
                let message = error_message(body).map(|m| scrub(&m, key));
                if status == 404
                    && message.as_deref().unwrap_or(body).to_lowercase().contains("model")
                {
                    return BackendError::Other(format!(
                        "The model \"{}\" isn't available at {}.",
                        self.model,
                        self.base()
                    ));
                }
                BackendError::Other(message.unwrap_or_else(|| {
                    format!("The model server answered with an error (HTTP {status}).")
                }))
            }
        }
    }
}

/// `{"error":{"message":"…"}}` or Ollama-style `{"error":"…"}`.
fn error_message(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let e = v.get("error")?;
    e.get("message")
        .and_then(Value::as_str)
        .or_else(|| e.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn parse_completion(body: &str) -> Result<Completion, BackendError> {
    let bad = || BackendError::Other("The model sent a reply InfinaBox couldn't read.".to_string());
    let v: Value = serde_json::from_str(body).map_err(|_| bad())?;
    let choice = v.get("choices").and_then(|c| c.get(0)).ok_or_else(bad)?;
    let message = choice.get("message").ok_or_else(bad)?;

    // `content` is a string or null (a few servers send a list of parts).
    let text = match message.get("content") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(parts)) => Some(
            parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<String>(),
        ),
        _ => None,
    }
    .filter(|s| !s.is_empty());

    let mut tool_calls = Vec::new();
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (i, c) in calls.iter().enumerate() {
            let f = c.get("function").unwrap_or(c);
            let Some(name) = f.get("name").and_then(Value::as_str) else { continue };
            let arguments = match f.get("arguments") {
                // Normal: a JSON string. Malformed JSON is handed back raw so
                // the loop can tell the model what it sent.
                Some(Value::String(s)) if s.trim().is_empty() => json!({}),
                Some(Value::String(s)) => {
                    serde_json::from_str(s).unwrap_or_else(|_| json!({"_raw": s}))
                }
                // Some local servers already send an object.
                Some(v @ Value::Object(_)) => v.clone(),
                _ => json!({}),
            };
            let id = c
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("call_{i}"));
            tool_calls.push(ToolCall { id, name: name.to_string(), arguments });
        }
    }

    let mut stop = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("stop") => StopReason::EndTurn,
        Some("tool_calls") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        _ => StopReason::Other,
    };
    // Some local servers say "stop" even when they asked for tools.
    if stop == StopReason::EndTurn && !tool_calls.is_empty() {
        stop = StopReason::ToolUse;
    }

    let usage = v.get("usage").filter(|u| u.is_object()).map(|u| Usage {
        input_tokens: u.get("prompt_tokens").and_then(Value::as_u64),
        output_tokens: u.get("completion_tokens").and_then(Value::as_u64),
    });

    Ok(Completion { text, tool_calls, usage, stop })
}

impl ChatBackend for OpenAiCompatible {
    fn label(&self) -> String {
        self.label.clone()
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
        let url = format!("{}/chat/completions", self.base());
        match http_call("POST", url, self.headers(), Some(body), REQUEST_TIMEOUT, cancel) {
            Ok(r) if (200..300).contains(&r.status) => parse_completion(&r.body),
            Ok(r) => Err(self.classify_error(r.status, &r.body)),
            Err(TransportError::Cancelled) => Err(BackendError::Cancelled),
            Err(TransportError::Unreachable) => Err(self.unreachable()),
            Err(TransportError::Other(m)) => Err(BackendError::Other(m)),
        }
    }

    fn check(&self) -> Option<bool> {
        let url = format!("{}/models", self.base());
        let never = AtomicBool::new(false);
        match http_call("GET", url, self.headers(), None, CHECK_TIMEOUT, &never) {
            Ok(r) if r.status == 200 => Some(true),
            Ok(r) if r.status == 401 || r.status == 403 => Some(false),
            _ => None,
        }
    }
}

// ---- tests ------------------------------------------------------------------

/// A scripted local HTTP server standing in for a provider (no live network).
#[cfg(test)]
pub(super) mod fake {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    pub struct Seen {
        pub method: String,
        pub path: String,
        pub headers: Vec<(String, String)>,
        pub body: String,
    }
    impl Seen {
        pub fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        }
    }

    pub struct Reply {
        pub status: u16,
        pub body: String,
        pub delay: Duration,
    }
    pub fn reply(status: u16, body: &str) -> Reply {
        Reply { status, body: body.to_string(), delay: Duration::ZERO }
    }

    /// Serves `replies` in order, one per request. Returns the base URL
    /// (`http://127.0.0.1:port`) and the requests seen so far.
    pub fn serve(replies: Vec<Reply>) -> (String, Arc<Mutex<Vec<Seen>>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr().to_ip().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for reply in replies {
                let Ok(mut req) = server.recv() else { return };
                let mut body = String::new();
                let _ = std::io::Read::read_to_string(req.as_reader(), &mut body);
                seen2.lock().unwrap().push(Seen {
                    method: req.method().to_string(),
                    path: req.url().to_string(),
                    headers: req
                        .headers()
                        .iter()
                        .map(|h| (h.field.to_string(), h.value.to_string()))
                        .collect(),
                    body,
                });
                std::thread::sleep(reply.delay);
                let resp = tiny_http::Response::from_string(reply.body)
                    .with_status_code(reply.status)
                    .with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                    );
                let _ = req.respond(resp);
            }
        });
        (base, seen)
    }

    /// A URL nothing listens on (the port was just released).
    pub fn dead_base() -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        drop(l);
        format!("http://{addr}")
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{Reply, dead_base, reply, serve};
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    const KEY: &str = "test-key-123";

    fn backend(base: &str, key: Option<&str>) -> OpenAiCompatible {
        OpenAiCompatible {
            base_url: format!("{base}/v1"),
            api_key: key.map(str::to_string),
            model: "test-model".into(),
            label: "Local model".into(),
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
        b: &OpenAiCompatible,
        msgs: &[Message],
        tools: &[ToolSpec],
    ) -> Result<Completion, BackendError> {
        b.complete("You are helpful.", msgs, tools, &AtomicBool::new(false))
    }

    #[test]
    fn sends_exact_request_and_reads_text() {
        let (base, seen) = serve(vec![reply(
            200,
            r#"{"choices":[{"message":{"role":"assistant","content":"Hello!"},"finish_reason":"stop"}],
                "usage":{"prompt_tokens":11,"completion_tokens":3}}"#,
        )]);
        let b = backend(&base, Some(KEY));
        let msgs = vec![
            Message::User("hi".into()),
            Message::Assistant {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "c1".into(),
                    name: "read_file".into(),
                    arguments: json!({"path":"a.gd"}),
                }],
            },
            Message::ToolResult {
                call_id: "c1".into(),
                content: "extends Node".into(),
                is_error: false,
            },
        ];
        let done = run(&b, &msgs, &[tool()]).unwrap();
        assert_eq!(done.text.as_deref(), Some("Hello!"));
        assert_eq!(done.stop, StopReason::EndTurn);
        assert!(done.tool_calls.is_empty());
        let u = done.usage.unwrap();
        assert_eq!((u.input_tokens, u.output_tokens), (Some(11), Some(3)));

        let seen = seen.lock().unwrap();
        let s = &seen[0];
        assert_eq!(s.method, "POST");
        assert_eq!(s.path, "/v1/chat/completions");
        assert_eq!(s.header("content-type"), Some("application/json"));
        assert_eq!(s.header("authorization"), Some("Bearer test-key-123"));
        assert_eq!(s.header("user-agent"), Some("InfinaBox"));
        let body: Value = serde_json::from_str(&s.body).unwrap();
        assert_eq!(
            body,
            json!({
                "model": "test-model",
                "messages": [
                    {"role":"system","content":"You are helpful."},
                    {"role":"user","content":"hi"},
                    {"role":"assistant","content":null,"tool_calls":[
                        {"id":"c1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"a.gd\"}"}}
                    ]},
                    {"role":"tool","tool_call_id":"c1","content":"extends Node"}
                ],
                "tools": [{"type":"function","function":{
                    "name":"read_file","description":"Read a file",
                    "parameters": {"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}
                }}],
                "tool_choice": "auto"
            })
        );
    }

    #[test]
    fn no_key_no_auth_header_and_no_tools_field() {
        let (base, seen) = serve(vec![reply(
            200,
            r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}"#,
        )]);
        let done = run(&backend(&base, None), &[Message::User("x".into())], &[]).unwrap();
        assert_eq!(done.text.as_deref(), Some("ok"));
        assert!(done.usage.is_none());
        let seen = seen.lock().unwrap();
        assert!(seen[0].header("authorization").is_none());
        let body: Value = serde_json::from_str(&seen[0].body).unwrap();
        assert!(body.get("tools").is_none() && body.get("tool_choice").is_none());
    }

    #[test]
    fn tool_call_response_with_string_and_object_arguments() {
        let (base, _) = serve(vec![reply(
            200,
            r#"{"choices":[{"message":{"content":null,"tool_calls":[
                {"id":"a","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"x.gd\"}"}},
                {"id":"b","type":"function","function":{"name":"list_dir","arguments":{"path":"."}}},
                {"type":"function","function":{"name":"noargs","arguments":""}}
            ]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":5,"completion_tokens":7}}"#,
        )]);
        let done = run(&backend(&base, None), &[Message::User("x".into())], &[tool()]).unwrap();
        assert_eq!(done.stop, StopReason::ToolUse);
        assert_eq!(done.text, None);
        assert_eq!(done.tool_calls.len(), 3);
        assert_eq!(
            done.tool_calls[0],
            ToolCall { id: "a".into(), name: "read_file".into(), arguments: json!({"path":"x.gd"}) }
        );
        assert_eq!(done.tool_calls[1].arguments, json!({"path":"."}));
        assert_eq!(done.tool_calls[2].id, "call_2");
        assert_eq!(done.tool_calls[2].arguments, json!({}));
    }

    #[test]
    fn malformed_arguments_are_passed_raw() {
        let (base, _) = serve(vec![reply(
            200,
            r#"{"choices":[{"message":{"tool_calls":[
                {"id":"a","function":{"name":"read_file","arguments":"{\"path\": "}}
            ]},"finish_reason":"tool_calls"}]}"#,
        )]);
        let done = run(&backend(&base, None), &[Message::User("x".into())], &[tool()]).unwrap();
        assert_eq!(done.tool_calls[0].arguments, json!({"_raw": "{\"path\": "}));
    }

    #[test]
    fn finish_reasons() {
        let (base, _) = serve(vec![
            reply(200, r#"{"choices":[{"message":{"content":"a"},"finish_reason":"length"}]}"#),
            reply(
                200,
                r#"{"choices":[{"message":{"content":"a"},"finish_reason":"content_filter"}]}"#,
            ),
        ]);
        let b = backend(&base, None);
        let m = [Message::User("x".into())];
        assert_eq!(run(&b, &m, &[]).unwrap().stop, StopReason::MaxTokens);
        assert_eq!(run(&b, &m, &[]).unwrap().stop, StopReason::Other);
    }

    #[test]
    fn error_statuses() {
        let (base, _) = serve(vec![
            reply(401, r#"{"error":{"message":"Incorrect API key provided: test-key-123"}}"#),
            reply(403, "{}"),
            reply(429, r#"{"error":{"message":"slow down"}}"#),
            reply(
                404,
                r#"{"error":{"message":"model 'test-model' not found, try pulling it first"}}"#,
            ),
            reply(500, r#"{"error":{"message":"the server fell over"}}"#),
            reply(400, r#"{"error":"bad thing test-key-123"}"#),
            reply(502, "<html>oops</html>"),
        ]);
        let b = backend(&base, Some(KEY));
        let m = [Message::User("x".into())];
        let e401 = run(&b, &m, &[]).unwrap_err();
        assert!(matches!(e401, BackendError::NotAuthenticated(_)));
        assert!(matches!(run(&b, &m, &[]).unwrap_err(), BackendError::NotAuthenticated(_)));
        assert!(matches!(run(&b, &m, &[]).unwrap_err(), BackendError::RateLimited(_)));
        match run(&b, &m, &[]).unwrap_err() {
            BackendError::Other(msg) => {
                assert_eq!(msg, format!("The model \"test-model\" isn't available at {base}/v1."))
            }
            e => panic!("{e:?}"),
        }
        assert_eq!(
            run(&b, &m, &[]).unwrap_err(),
            BackendError::Other("the server fell over".into())
        );
        // A server that echoes the key gets it scrubbed.
        let echoed = run(&b, &m, &[]).unwrap_err();
        assert_eq!(echoed, BackendError::Other("bad thing …".into()));
        match run(&b, &m, &[]).unwrap_err() {
            BackendError::Other(msg) => assert!(msg.contains("502")),
            e => panic!("{e:?}"),
        }
        for e in [e401, echoed] {
            assert!(!format!("{e:?}").contains(KEY));
        }
    }

    #[test]
    fn connection_refused_is_unreachable() {
        let base = dead_base();
        let e = run(&backend(&base, Some(KEY)), &[Message::User("x".into())], &[]).unwrap_err();
        assert_eq!(
            e,
            BackendError::Unreachable(format!(
                "Couldn't reach the model at {base}/v1. Is it running?"
            ))
        );
        assert!(!format!("{e:?}").contains(KEY));
    }

    #[test]
    fn cancel_while_server_sleeps() {
        let (base, _) = serve(vec![Reply {
            status: 200,
            body: "{}".into(),
            delay: Duration::from_secs(5),
        }]);
        let b = backend(&base, None);
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
        // Already-set flag: returns without a request.
        assert_eq!(
            b.complete("s", &[Message::User("x".into())], &[], &cancel).unwrap_err(),
            BackendError::Cancelled
        );
    }

    #[test]
    fn check_maps_statuses() {
        let (base, seen) =
            serve(vec![reply(200, "{}"), reply(401, "{}"), reply(403, "{}"), reply(500, "{}")]);
        let b = backend(&base, Some(KEY));
        assert_eq!(b.check(), Some(true));
        assert_eq!(b.check(), Some(false));
        assert_eq!(b.check(), Some(false));
        assert_eq!(b.check(), None);
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].method, "GET");
        assert_eq!(seen[0].path, "/v1/models");
        assert_eq!(seen[0].header("authorization"), Some("Bearer test-key-123"));
        assert_eq!(backend(&dead_base(), None).check(), None);
    }

    #[test]
    fn debug_hides_the_key() {
        let b = backend("http://x", Some(KEY));
        let d = format!("{b:?}");
        assert!(!d.contains(KEY));
        assert!(d.contains("<hidden>"));
    }
}
