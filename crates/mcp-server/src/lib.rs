//! The InfinaBox MCP server (spec §8.4): the tools the user's own agent CLI
//! gets for this project — Context cards, running the game and reading its
//! errors (via the bridge to the running app), and snapshot history.
//!
//! There is no separate binary to ship: the InfinaBox app executable runs
//! this when launched as `<app exe> --mcp-server` (see `src-tauri/src/main.rs`),
//! and `infinabox-cli mcp-server` does the same for development.
//!
//! The server reads its project and bridge location from the environment:
//! `INFINABOX_PROJECT`, `INFINABOX_BRIDGE_ADDR`, `INFINABOX_BRIDGE_TOKEN`.

pub mod bridge_protocol;

/// Environment variable names shared with the app (which sets them when it
/// launches the agent).
pub const ENV_PROJECT: &str = "INFINABOX_PROJECT";
pub const ENV_BRIDGE_ADDR: &str = "INFINABOX_BRIDGE_ADDR";
pub const ENV_BRIDGE_TOKEN: &str = "INFINABOX_BRIDGE_TOKEN";

/// The command-line flag that makes the app executable run this server
/// instead of its window.
pub const MCP_SERVER_FLAG: &str = "--mcp-server";

/// Serves MCP over stdin/stdout until the client disconnects. Blocks.
/// Phase A Task B fills this in.
pub fn run_stdio() -> anyhow::Result<()> {
    anyhow::bail!("not implemented yet: infinabox_mcp_server::run_stdio")
}
