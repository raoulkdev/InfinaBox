//! The MCP tool surface: thin `rmcp` wrappers over `context.rs`,
//! `bridge_client.rs`, and `infinabox_core::snapshot`.
//!
//! Tool results are plain text (JSON for structured data). A tool that
//! fails returns an MCP tool error (`isError: true`) carrying a readable
//! message, never a made-up success.

use std::path::PathBuf;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::bridge_client::{self, BridgeConfig};
use crate::bridge_protocol::BridgeRequest;
use crate::context::ContextDir;

/// The server name agent CLIs see (Claude Code prefixes tools as
/// `mcp__infinabox__<tool>`).
pub const SERVER_NAME: &str = "infinabox";

/// Every tool this server exposes, in the order they're declared below.
/// Used by tests to check the real `tools/list` output.
pub const TOOL_NAMES: &[&str] = &[
    "list_context_cards",
    "read_context_card",
    "search_context",
    "write_context_card",
    "run_game",
    "stop_game",
    "get_game_status",
    "get_game_errors",
    "get_game_output",
    "list_snapshots",
];

const INSTRUCTIONS: &str = "Tools for the InfinaBox game project you are working in. \
Context cards (markdown in .ibproject/context/) hold the game's design: read or search them \
before making design decisions, and update them when a decision changes. The game tools run \
the Godot game inside the InfinaBox app and read its errors and output; use them to check \
your changes actually work. list_snapshots shows the project's saved history.";

const DEFAULT_ERROR_LIMIT: usize = 20;
const DEFAULT_OUTPUT_LINES: usize = 100;
const DEFAULT_SNAPSHOT_LIMIT: usize = 20;

#[derive(Deserialize, JsonSchema)]
pub struct CardPath {
    /// Path of the card inside the context folder, e.g. "mechanics/double-jump.md".
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct WriteCard {
    /// Path of the card inside the context folder, ending in .md, e.g. "characters/boss.md".
    pub path: String,
    /// The card's full new markdown content. Replaces the whole file.
    pub markdown: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchQuery {
    /// Text to look for (case-insensitive, plain text, not a regex).
    pub query: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ErrorsParams {
    /// How many of the most recent errors to return. Defaults to 20.
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct OutputParams {
    /// How many of the most recent output lines to return. Defaults to 100.
    pub lines: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SnapshotParams {
    /// How many snapshots to return, newest first. Defaults to 20.
    pub limit: Option<usize>,
}

#[derive(Clone)]
pub struct InfinaBoxServer {
    /// `None` when `INFINABOX_PROJECT` isn't set; project tools then say so.
    project: Option<PathBuf>,
    bridge: Option<BridgeConfig>,
    tool_router: ToolRouter<Self>,
}

impl InfinaBoxServer {
    pub fn new(project: Option<PathBuf>, bridge: Option<BridgeConfig>) -> Self {
        Self {
            project,
            bridge,
            tool_router: Self::tool_router(),
        }
    }

    /// Reads `INFINABOX_PROJECT`, `INFINABOX_BRIDGE_ADDR`, `INFINABOX_BRIDGE_TOKEN`.
    pub fn from_env() -> Self {
        let project = std::env::var_os(crate::ENV_PROJECT)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        Self::new(project, BridgeConfig::from_env())
    }

    fn project(&self) -> Result<&PathBuf, String> {
        self.project.as_ref().ok_or_else(|| {
            format!(
                "No project is set: this MCP server needs {} to point at the game's folder \
                 (InfinaBox sets it when it starts the agent).",
                crate::ENV_PROJECT
            )
        })
    }

    fn context(&self) -> Result<ContextDir, String> {
        Ok(ContextDir::for_project(self.project()?))
    }

    async fn bridge(&self, request: BridgeRequest) -> Result<String, String> {
        let data = bridge_client::call(self.bridge.as_ref(), &request).await?;
        Ok(to_json(&data))
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("(unserializable: {e})"))
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[tool_router]
impl InfinaBoxServer {
    #[tool(
        description = "List every Context card in this game project (the markdown design \
notes in .ibproject/context/). Use this first to see what's already decided about the game."
    )]
    async fn list_context_cards(&self) -> Result<String, String> {
        let cards = self.context()?.list().map_err(err)?;
        Ok(to_json(&cards))
    }

    #[tool(
        description = "Read one Context card's full markdown. Use a path from \
list_context_cards or search_context."
    )]
    async fn read_context_card(
        &self,
        Parameters(CardPath { path }): Parameters<CardPath>,
    ) -> Result<String, String> {
        self.context()?.read(&path).map_err(err)
    }

    #[tool(
        description = "Search all Context cards for a word or phrase (case-insensitive plain \
text). Returns matching lines with card path and line number. Use it to find what the design \
says about a topic before changing related code."
    )]
    async fn search_context(
        &self,
        Parameters(SearchQuery { query }): Parameters<SearchQuery>,
    ) -> Result<String, String> {
        let hits = self.context()?.search(&query).map_err(err)?;
        Ok(to_json(&hits))
    }

    #[tool(
        description = "Create or replace a Context card with new markdown. Use it to record a \
design decision or keep a card up to date after changing the game. Paths must end in .md and \
stay inside the context folder."
    )]
    async fn write_context_card(
        &self,
        Parameters(WriteCard { path, markdown }): Parameters<WriteCard>,
    ) -> Result<String, String> {
        let written = self.context()?.write(&path, &markdown).map_err(err)?;
        Ok(format!("Saved context card {written}."))
    }

    #[tool(
        description = "Run the game in the InfinaBox app (restarts it if already running). \
Use after changing scripts or scenes, then check get_game_errors."
    )]
    async fn run_game(&self) -> Result<String, String> {
        self.bridge(BridgeRequest::RunGame).await
    }

    #[tool(description = "Stop the running game in the InfinaBox app.")]
    async fn stop_game(&self) -> Result<String, String> {
        self.bridge(BridgeRequest::StopGame).await
    }

    #[tool(description = "Check whether the game is running in the InfinaBox app, and its state.")]
    async fn get_game_status(&self) -> Result<String, String> {
        self.bridge(BridgeRequest::GameStatus).await
    }

    #[tool(
        description = "Get the most recent errors from the running (or last run) game, with \
file and line where known. Use after run_game to see if your change broke anything."
    )]
    async fn get_game_errors(
        &self,
        Parameters(ErrorsParams { limit }): Parameters<ErrorsParams>,
    ) -> Result<String, String> {
        self.bridge(BridgeRequest::RecentErrors {
            limit: limit.unwrap_or(DEFAULT_ERROR_LIMIT),
        })
        .await
    }

    #[tool(
        description = "Get the most recent lines the game printed (print() output and engine \
messages). Use to check what the game did at runtime."
    )]
    async fn get_game_output(
        &self,
        Parameters(OutputParams { lines }): Parameters<OutputParams>,
    ) -> Result<String, String> {
        self.bridge(BridgeRequest::RecentOutput {
            lines: lines.unwrap_or(DEFAULT_OUTPUT_LINES),
        })
        .await
    }

    #[tool(
        description = "List the project's saved snapshots (history points), newest first, \
with title, time, and files changed. Read-only: InfinaBox creates snapshots itself."
    )]
    async fn list_snapshots(
        &self,
        Parameters(SnapshotParams { limit }): Parameters<SnapshotParams>,
    ) -> Result<String, String> {
        let project = self.project()?.clone();
        let limit = limit.unwrap_or(DEFAULT_SNAPSHOT_LIMIT);
        // git2 work is blocking; keep it off the async executor.
        let snapshots = tokio::task::spawn_blocking(move || {
            infinabox_core::snapshot::list_snapshots(&project, limit)
        })
        .await
        .map_err(|e| format!("listing snapshots failed: {e}"))?
        .map_err(err)?;
        Ok(to_json(&snapshots))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for InfinaBoxServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-mcp-srv-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn router_exposes_exactly_the_documented_tools() {
        let server = InfinaBoxServer::new(None, None);
        let mut names: Vec<String> = server
            .tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        names.sort();
        let mut expected: Vec<String> = TOOL_NAMES.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(names, expected);
    }

    #[tokio::test]
    async fn context_tools_work_against_a_temp_project() {
        let project = temp_project();
        let server = InfinaBoxServer::new(Some(project.clone()), None);

        assert_eq!(server.list_context_cards().await.unwrap(), "[]");
        let saved = server
            .write_context_card(Parameters(WriteCard {
                path: "mechanics/double-jump.md".into(),
                markdown: "# Double jump\nPress jump twice.\n".into(),
            }))
            .await
            .unwrap();
        assert_eq!(saved, "Saved context card mechanics/double-jump.md.");
        assert!(
            project
                .join(".ibproject/context/mechanics/double-jump.md")
                .is_file()
        );

        let list: Vec<String> =
            serde_json::from_str(&server.list_context_cards().await.unwrap()).unwrap();
        assert_eq!(list, vec!["mechanics/double-jump.md"]);

        let text = server
            .read_context_card(Parameters(CardPath {
                path: "mechanics/double-jump.md".into(),
            }))
            .await
            .unwrap();
        assert_eq!(text, "# Double jump\nPress jump twice.\n");

        let hits: serde_json::Value = serde_json::from_str(
            &server
                .search_context(Parameters(SearchQuery {
                    query: "JUMP TWICE".into(),
                }))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            hits,
            serde_json::json!([{ "path": "mechanics/double-jump.md", "line": 2, "text": "Press jump twice." }])
        );

        let escape = server
            .read_context_card(Parameters(CardPath {
                path: "../../etc/passwd".into(),
            }))
            .await
            .unwrap_err();
        assert!(escape.contains("inside the context folder"), "{escape}");

        std::fs::remove_dir_all(project).ok();
    }

    #[tokio::test]
    async fn tools_without_a_project_say_so() {
        let server = InfinaBoxServer::new(None, None);
        let e = server.list_context_cards().await.unwrap_err();
        assert!(e.contains(crate::ENV_PROJECT), "{e}");
        let e = server
            .list_snapshots(Parameters(SnapshotParams { limit: None }))
            .await
            .unwrap_err();
        assert!(e.contains(crate::ENV_PROJECT), "{e}");
    }

    #[tokio::test]
    async fn game_tools_without_the_app_say_it_is_not_running() {
        let server = InfinaBoxServer::new(Some(temp_project()), None);
        for result in [
            server.run_game().await,
            server.stop_game().await,
            server.get_game_status().await,
            server
                .get_game_errors(Parameters(ErrorsParams { limit: Some(5) }))
                .await,
            server
                .get_game_output(Parameters(OutputParams { lines: None }))
                .await,
        ] {
            let e = result.unwrap_err();
            assert!(e.starts_with(bridge_client::APP_NOT_RUNNING), "{e}");
        }
    }

    /// Calls straight through to `infinabox_core::snapshot::list_snapshots`
    /// and surfaces whatever it returns. The result shape isn't asserted:
    /// that's Task D's function, and this test must hold whether it's the
    /// Wave 0 stub (an error) or the real implementation.
    #[tokio::test]
    async fn list_snapshots_surfaces_core_result() {
        let project = temp_project();
        let server = InfinaBoxServer::new(Some(project.clone()), None);
        let got = server
            .list_snapshots(Parameters(SnapshotParams { limit: Some(3) }))
            .await;
        let direct = infinabox_core::snapshot::list_snapshots(&project, 3);
        match (got, direct) {
            (Ok(text), Ok(snaps)) => assert_eq!(text, to_json(&snaps)),
            (Err(text), Err(e)) => assert_eq!(text, format!("{e:#}")),
            (got, direct) => panic!("tool {got:?} disagrees with core {direct:?}"),
        }
        std::fs::remove_dir_all(project).ok();
    }
}
