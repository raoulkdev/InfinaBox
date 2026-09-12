//! Stand-in for a real Godot MCP server, since Godot isn't installed on this
//! machine yet. Speaks just enough of the request/response shape over stdio
//! to prove the round-trip mechanics that `mcp_client.rs` depends on. Swap
//! the spawned command for a real Godot MCP server once one is available —
//! `mcp_client.rs` shouldn't need to change materially.

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

fn main() {
    let mut line = String::new();
    let stdin = io::stdin();

    let response = match stdin.lock().read_line(&mut line) {
        Ok(0) => {
            // No input at all (EOF immediately) — treat like malformed input.
            error_response(Value::Null, -32600, "empty request: no input received")
        }
        Ok(_) => match serde_json::from_str::<Value>(line.trim()) {
            Ok(request) => handle_request(&request),
            Err(err) => error_response(Value::Null, -32700, &format!("parse error: {err}")),
        },
        Err(err) => error_response(Value::Null, -32603, &format!("failed to read stdin: {err}")),
    };

    let mut stdout = io::stdout();
    // Best-effort write; if this fails there's no one left to report to.
    let _ = writeln!(stdout, "{response}");
    let _ = stdout.flush();
}

fn handle_request(request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);

    let method = match request.get("method").and_then(Value::as_str) {
        Some(m) => m,
        None => return error_response(id, -32600, "invalid request: missing \"method\""),
    };

    if method != "tools/call" {
        return error_response(id, -32601, &format!("method not found: {method}"));
    }

    let tool_name = request
        .get("params")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str);

    match tool_name {
        Some("get_open_scene") => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "scene": "res://scenes/Sector3_Elevator.tscn",
                "engine": "Godot 4.3 (mock)"
            }
        }),
        Some(other) => error_response(id, -32602, &format!("unknown tool: {other}")),
        None => error_response(id, -32602, "invalid params: missing \"params.name\""),
    }
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}
