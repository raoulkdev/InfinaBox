//! Milestone 6 (Phase 0): prove a live connection to "the editor" works —
//! spawn a server process, speak a request/response protocol to it over
//! stdio, and get back a correct, live answer. No Godot install on this
//! machine yet, so this round-trips against `mock_godot_mcp` (see
//! src/bin/mock_godot_mcp.rs) standing in for a real Godot MCP server; the
//! client-side plumbing here is what a real one would use unchanged.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

pub fn ping(server_cmd: &str) -> Result<()> {
    let mut child = Command::new(server_cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn MCP server command: {server_cmd}"))?;

    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "get_open_scene" }
    });

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("failed to open stdin of MCP server process"))?;
        writeln!(stdin, "{request}")
            .with_context(|| "failed to write request to MCP server stdin")?;
        stdin.flush().with_context(|| "failed to flush MCP server stdin")?;
        // stdin is dropped (and thus closed) here, signaling EOF to the child.
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to open stdout of MCP server process"))?;
    let mut reader = BufReader::new(stdout);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .with_context(|| "failed to read response from MCP server stdout")?;

    let status = child
        .wait()
        .with_context(|| "failed to wait on MCP server process")?;
    if !status.success() {
        bail!("MCP server process exited with non-zero status: {status}");
    }

    if response_line.trim().is_empty() {
        bail!("MCP server produced no response on stdout");
    }

    let response: Value = serde_json::from_str(response_line.trim())
        .with_context(|| format!("failed to parse MCP server response as JSON: {response_line}"))?;

    if let Some(error) = response.get("error") {
        bail!("Godot MCP server returned an error: {error}");
    }

    let result = response
        .get("result")
        .ok_or_else(|| anyhow!("MCP server response had neither \"result\" nor \"error\": {response}"))?;

    let scene = result
        .get("scene")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("MCP server result missing \"scene\" field: {result}"))?;
    let engine = result
        .get("engine")
        .and_then(Value::as_str)
        .unwrap_or("unknown engine");

    println!("Godot reports the open scene is: {scene} (via {engine})");

    Ok(())
}
