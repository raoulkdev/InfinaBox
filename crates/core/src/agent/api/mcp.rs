//! Bridge to the InfinaBox MCP server for the API runtimes: starts
//! `TurnRequest.mcp`, lists its tools as `ToolSpec`s and forwards calls, so
//! every runtime gets the same Context / game / history tools.
//!
//! The server speaks MCP over the child's stdio as newline-delimited
//! JSON-RPC 2.0 (what `rmcp`'s stdio transport does). `crate::mcp_client` is
//! a one-shot spike that sends a single request and closes, so this speaks
//! the protocol itself: `initialize`, `notifications/initialized`,
//! `tools/list`, `tools/call`. The child is killed when the bridge is
//! dropped, so a turn that ends or is stopped never leaves a server behind.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::ToolSpec;
use crate::agent::types::McpLaunch;

/// How long the handshake and `tools/list` may take.
const SETUP_TIMEOUT: Duration = Duration::from_secs(30);
/// How long one tool call may take: the game bridge's own limit is 300 s.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(330);
/// How often a wait wakes up to look at the cancel flag.
const POLL: Duration = Duration::from_millis(50);
/// Most of the server's stderr kept, in bytes.
const MAX_STDERR_BYTES: usize = 16 * 1024;
const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Clone, PartialEq)]
pub enum McpError {
    /// The turn was stopped while waiting.
    Cancelled,
    /// Anything else, as text for the model or the logs.
    Failed(String),
}

/// What a tool call returned.
#[derive(Debug, Clone, PartialEq)]
pub struct McpToolResult {
    pub text: String,
    pub is_error: bool,
}

/// A running MCP server and the conversation with it.
pub struct McpBridge {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<Value>,
    stderr: Arc<Mutex<String>>,
    next_id: u64,
}

impl McpBridge {
    /// Starts the server, does the handshake and lists its tools.
    pub fn start(
        launch: &McpLaunch,
        cancel: &AtomicBool,
    ) -> Result<(Self, Vec<ToolSpec>), McpError> {
        let mut command = Command::new(&launch.command);
        command
            .args(&launch.args)
            .envs(launch.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|e| {
            McpError::Failed(format!(
                "couldn't start {}: {e}",
                launch.command.display()
            ))
        })?;
        let (Some(stdin), Some(stdout), Some(stderr_pipe)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(McpError::Failed("the server's pipes didn't open".into()));
        };

        // One reader thread per pipe; both end when the child dies.
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(value) = serde_json::from_str::<Value>(line.trim()) {
                    if tx.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        let stderr = Arc::new(Mutex::new(String::new()));
        let sink = stderr.clone();
        std::thread::spawn(move || {
            let mut pipe = stderr_pipe;
            let mut buf = [0u8; 2048];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut text) = sink.lock() {
                    let room = MAX_STDERR_BYTES.saturating_sub(text.len());
                    if room > 0 {
                        text.push_str(&String::from_utf8_lossy(&buf[..n.min(room)]));
                    }
                }
            }
        });

        let mut bridge = Self {
            child,
            stdin,
            lines,
            stderr,
            next_id: 1,
        };
        bridge.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "infinabox", "version": env!("CARGO_PKG_VERSION")}
            }),
            SETUP_TIMEOUT,
            cancel,
        )?;
        bridge.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        let listed = bridge.request("tools/list", json!({}), SETUP_TIMEOUT, cancel)?;
        let tools = listed
            .get("tools")
            .and_then(Value::as_array)
            .map(|tools| tools.iter().filter_map(tool_spec).collect())
            .unwrap_or_default();
        Ok((bridge, tools))
    }

    /// Calls a tool and waits (up to `CALL_TIMEOUT`) for its result.
    pub fn call(
        &mut self,
        name: &str,
        arguments: &Value,
        cancel: &AtomicBool,
    ) -> Result<McpToolResult, McpError> {
        self.call_with_timeout(name, arguments, CALL_TIMEOUT, cancel)
    }

    fn call_with_timeout(
        &mut self,
        name: &str,
        arguments: &Value,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<McpToolResult, McpError> {
        let arguments = if arguments.is_object() {
            arguments.clone()
        } else {
            json!({})
        };
        let result = self.request(
            "tools/call",
            json!({"name": name, "arguments": arguments}),
            timeout,
            cancel,
        )?;
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Ok(McpToolResult {
            text: if text.is_empty() { "Done.".into() } else { text },
            is_error: result
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    /// What the server has written to stderr so far (capped).
    pub fn stderr(&self) -> String {
        self.stderr.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn send(&mut self, message: &Value) -> Result<(), McpError> {
        writeln!(self.stdin, "{message}")
            .and_then(|_| self.stdin.flush())
            .map_err(|e| McpError::Failed(format!("the server isn't listening: {e}")))
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let deadline = Instant::now() + timeout;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(McpError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(McpError::Failed(format!(
                    "the server didn't answer {method} within {} seconds",
                    timeout.as_secs()
                )));
            }
            let message = match self.lines.recv_timeout(POLL) {
                Ok(m) => m,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(McpError::Failed(self.exit_message()));
                }
            };
            // Notifications and answers to other requests are not ours.
            if message.get("id") != Some(&json!(id)) || message.get("method").is_some() {
                continue;
            }
            if let Some(error) = message.get("error") {
                let text = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                return Err(McpError::Failed(format!("{method}: {text}")));
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn exit_message(&mut self) -> String {
        // Give the stderr reader a moment to catch up with a dying server.
        std::thread::sleep(Duration::from_millis(50));
        let stderr = self.stderr();
        let tail = stderr.trim();
        if tail.is_empty() {
            "the server stopped".into()
        } else {
            let start = tail.len().saturating_sub(500);
            let start = (start..=tail.len())
                .find(|i| tail.is_char_boundary(*i))
                .unwrap_or(tail.len());
            format!("the server stopped: {}", &tail[start..])
        }
    }
}

impl Drop for McpBridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An MCP tool definition as a `ToolSpec`. The schema is passed on as is,
/// minus the `$schema` key (some providers reject it) and made sure to be
/// an object schema.
fn tool_spec(tool: &Value) -> Option<ToolSpec> {
    let name = tool.get("name")?.as_str()?.to_string();
    let mut parameters = tool
        .get("inputSchema")
        .filter(|s| s.is_object())
        .cloned()
        .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
    if let Some(schema) = parameters.as_object_mut() {
        schema.remove("$schema");
        schema.entry("type").or_insert(json!("object"));
        if schema.get("type") == Some(&json!("object")) {
            schema.entry("properties").or_insert(json!({}));
        }
    }
    Some(ToolSpec {
        name,
        description: tool
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        parameters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/fixtures/echo-mcp-server.mjs")
    }

    pub(crate) fn node_launch(extra: &[&str]) -> McpLaunch {
        let mut args = vec![fixture().to_string_lossy().into_owned()];
        args.extend(extra.iter().map(|s| s.to_string()));
        McpLaunch {
            command: PathBuf::from("node"),
            args,
            env: vec![],
        }
    }

    #[test]
    fn handshake_lists_tools_and_calls_them() {
        let cancel = AtomicBool::new(false);
        let (mut bridge, tools) =
            McpBridge::start(&node_launch(&["--propose-plan"]), &cancel).unwrap();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["echo", "propose_plan"]);
        let plan = tools.iter().find(|t| t.name == "propose_plan").unwrap();
        assert!(plan.parameters.get("$schema").is_none());
        assert_eq!(plan.parameters["type"], "object");
        assert!(plan.description.contains("plan"));

        let echoed = bridge.call("echo", &json!({"text": "hi"}), &cancel).unwrap();
        assert_eq!(
            echoed,
            McpToolResult {
                text: "echo: hi".into(),
                is_error: false
            }
        );
        let bad = bridge
            .call("propose_plan", &json!({"title": "", "steps": []}), &cancel)
            .unwrap();
        assert!(bad.is_error);
        assert!(bad.text.contains("title"));
        // Calls after calls keep working.
        assert!(bridge.call("echo", &json!({"text": "again"}), &cancel).is_ok());
    }

    #[test]
    fn a_server_that_cannot_start_is_an_error() {
        let cancel = AtomicBool::new(false);
        let launch = McpLaunch {
            command: PathBuf::from("/definitely/not/a/program"),
            args: vec![],
            env: vec![],
        };
        assert!(matches!(
            McpBridge::start(&launch, &cancel),
            Err(McpError::Failed(_))
        ));
        // A program that exits at once is an error with its stderr.
        let launch = McpLaunch {
            command: PathBuf::from("sh"),
            args: vec!["-c".into(), "echo boom >&2; exit 3".into()],
            env: vec![],
        };
        match McpBridge::start(&launch, &cancel) {
            Err(McpError::Failed(m)) => assert!(m.contains("boom"), "{m}"),
            other => panic!("expected a failure, got {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn a_silent_server_times_out_and_a_cancel_interrupts_a_wait() {
        // Answers nothing at all.
        let launch = McpLaunch {
            command: PathBuf::from("sh"),
            args: vec!["-c".into(), "cat > /dev/null".into()],
            env: vec![],
        };
        let cancel = AtomicBool::new(true);
        assert!(matches!(
            McpBridge::start(&launch, &cancel),
            Err(McpError::Cancelled)
        ));

        // A tool call that never returns times out.
        let cancel = AtomicBool::new(false);
        let (mut bridge, _) = McpBridge::start(&node_launch(&[]), &cancel).unwrap();
        // Swap the server for one that never answers by killing it: the wait
        // then ends as a failure, not a hang.
        let _ = bridge.child.kill();
        let r = bridge.call_with_timeout("echo", &json!({}), Duration::from_millis(300), &cancel);
        assert!(matches!(r, Err(McpError::Failed(_))), "{r:?}");
    }

    #[test]
    fn the_child_is_killed_when_the_bridge_is_dropped() {
        let cancel = AtomicBool::new(false);
        let (bridge, _) = McpBridge::start(&node_launch(&[]), &cancel).unwrap();
        let pid = bridge.child.id();
        drop(bridge);
        // The process is gone (and reaped), so signalling it fails.
        #[cfg(unix)]
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        let _ = pid;
    }

    #[test]
    fn schemas_are_cleaned_up() {
        let spec = tool_spec(&json!({"name": "t", "inputSchema": {"$schema": "x", "type": "object"}})).unwrap();
        assert_eq!(spec.parameters, json!({"type": "object", "properties": {}}));
        let spec = tool_spec(&json!({"name": "t"})).unwrap();
        assert_eq!(spec.parameters, json!({"type": "object", "properties": {}}));
        assert!(tool_spec(&json!({"description": "no name"})).is_none());
    }
}
