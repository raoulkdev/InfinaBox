//! Phase A contract (frozen — see the Phase A plan's "Wave 0 contracts"):
//! how this MCP server process talks to the running InfinaBox app.
//!
//! Loopback TCP, newline-delimited JSON. The app listens on
//! `127.0.0.1:<random port>` with a random per-launch token. The first line
//! on every connection must be a `Hello` carrying that token, or the app
//! closes the connection. After that, one `BridgeRequest` line gets one
//! `BridgeResponse` line, repeatedly.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hello {
    pub hello: String,
    /// The project this MCP server was launched for, so the app runs the
    /// right game.
    pub project: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum BridgeRequest {
    RunGame,
    StopGame,
    GameStatus,
    RecentErrors { limit: usize },
    RecentOutput { lines: usize },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BridgeResponse {
    Ok { data: serde_json::Value },
    Error { message: String },
}
