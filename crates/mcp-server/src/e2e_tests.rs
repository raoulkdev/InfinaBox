//! End-to-end: spawn the real `infinabox-cli mcp-server` binary and speak
//! raw MCP JSON-RPC to it over stdio, the way an agent CLI does.
//!
//! Locating the binary: the cli crate depends on this one, so its binary
//! can't be a `CARGO_BIN_EXE_*` of this crate's tests. Instead the test asks
//! cargo to build `infinabox-cli` (a no-op when it's up to date) and takes
//! the executable path from cargo's own JSON build messages, which stays
//! correct under any `CARGO_TARGET_DIR`. Set `INFINABOX_CLI_BIN` to skip the
//! build and use a specific binary.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::server::TOOL_NAMES;

fn cli_binary() -> PathBuf {
    if let Some(p) = std::env::var_os("INFINABOX_CLI_BIN") {
        return PathBuf::from(p);
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../cli/Cargo.toml");
    let out = Command::new(cargo)
        .args([
            "build",
            "--quiet",
            "--message-format=json",
            "--bin",
            "infinabox-cli",
        ])
        .arg("--manifest-path")
        .arg(&manifest)
        .stderr(Stdio::inherit())
        .output()
        .expect("run cargo build for infinabox-cli");
    assert!(out.status.success(), "building infinabox-cli failed");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|m| m["reason"] == "compiler-artifact" && m["target"]["name"] == "infinabox-cli")
        .find_map(|m| m["executable"].as_str().map(PathBuf::from))
        .expect("cargo reported no infinabox-cli executable")
}

struct Session {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
}

impl Session {
    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Next JSON-RPC message carrying `id`, skipping any notifications.
    fn response(&self, id: u64) -> Value {
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(20))
                .expect("MCP server didn't answer in time");
            let msg: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("non-JSON on stdout ({e}): {line}"));
            if msg["id"] == json!(id) {
                return msg;
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn cli_mcp_server_handshakes_and_lists_tools() {
    let project = std::env::temp_dir().join(format!("infinabox-mcp-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&project).unwrap();

    let mut child = Command::new(cli_binary())
        .arg("mcp-server")
        .env(crate::ENV_PROJECT, &project)
        .env_remove(crate::ENV_BRIDGE_ADDR)
        .env_remove(crate::ENV_BRIDGE_TOKEN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn infinabox-cli mcp-server");
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut s = Session {
        child,
        stdin,
        lines,
    };

    s.send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "infinabox-e2e-test", "version": "0" }
        }
    }));
    let init = s.response(1);
    assert_eq!(init["result"]["serverInfo"]["name"], "infinabox", "{init}");
    assert!(
        init["result"]["capabilities"]["tools"].is_object(),
        "{init}"
    );

    s.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    s.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    let list = s.response(2);
    let mut names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("no tools array: {list}"))
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    names.sort();
    let mut expected = TOOL_NAMES.to_vec();
    expected.sort();
    assert_eq!(names, expected);
    for tool in list["result"]["tools"].as_array().unwrap() {
        assert!(
            tool["description"].as_str().is_some_and(|d| !d.is_empty()),
            "tool without a description: {tool}"
        );
    }

    // A real tool call through the whole stack: with no app bridge in the
    // environment, the game tools must fail honestly.
    s.send(json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "get_game_status", "arguments": {} }
    }));
    let call = s.response(3);
    assert_eq!(call["result"]["isError"], true, "{call}");
    let text = call["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(
        text.starts_with(crate::bridge_client::APP_NOT_RUNNING),
        "{call}"
    );

    // And a context write lands in the project the env var named.
    s.send(json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": { "name": "write_context_card", "arguments": { "path": "concept.md", "markdown": "# Hi\n" } }
    }));
    let call = s.response(4);
    assert_ne!(call["result"]["isError"], true, "{call}");
    assert_eq!(
        std::fs::read_to_string(project.join(".ibproject/context/concept.md")).unwrap(),
        "# Hi\n"
    );

    drop(s);
    std::fs::remove_dir_all(project).ok();
}
