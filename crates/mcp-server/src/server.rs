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
    "propose_plan",
    "project_map",
    "find_symbol",
    "describe_scene",
];

/// What the agent is told after a plan was accepted.
pub const PLAN_SHOWN: &str = "The plan is now shown to the person with Approve and Change \
buttons. End your turn now with one short sentence, and don't change anything: you'll get a \
new message when they approve it or ask for changes.";

const INSTRUCTIONS: &str = "Tools for the InfinaBox game project you are working in. \
Context cards (markdown in .ibproject/context/) hold the game's design: read or search them \
before making design decisions, and update them when a decision changes. The game tools run \
the Godot game inside the InfinaBox app and read its errors and output; use them to check \
your changes actually work. list_snapshots shows the project's saved history. propose_plan \
shows the person a plan to approve before you change the game.";

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

#[derive(Deserialize, JsonSchema)]
pub struct PlanParams {
    /// A short title in plain words, at most 80 characters, e.g. "Add a double jump".
    pub title: String,
    // One doc line on purpose: schemars keeps a doc comment's line breaks.
    /// The steps in order, as plain sentences about what will change in the game (no code). 2-6 steps is best; at most 8, each at most 200 characters.
    pub steps: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SymbolParams {
    /// One GDScript name: a function, signal, variable, constant, class or autoload, e.g. "player_died" or "Events".
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SceneParams {
    /// A scene's path in the game, e.g. "scenes/main.tscn".
    pub path: String,
}

#[derive(Clone)]
pub struct InfinaBoxServer {
    /// `None` when `INFINABOX_PROJECT` isn't set; project tools then say so.
    project: Option<PathBuf>,
    bridge: Option<BridgeConfig>,
    /// Ask mode: tools that write are refused.
    read_only: bool,
    tool_router: ToolRouter<Self>,
}

impl InfinaBoxServer {
    pub fn new(project: Option<PathBuf>, bridge: Option<BridgeConfig>) -> Self {
        Self {
            project,
            bridge,
            read_only: false,
            tool_router: Self::tool_router(),
        }
    }

    /// A server that refuses every tool that writes (an Ask-mode turn).
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    fn writable(&self, what: &str) -> Result<(), String> {
        if self.read_only {
            return Err(format!(
                "This message is a question, so {what} isn't allowed. Answer from what you read; \
                 if something should change, say so and the person can switch to Build."
            ));
        }
        Ok(())
    }

    /// Reads `INFINABOX_PROJECT`, `INFINABOX_BRIDGE_ADDR`, `INFINABOX_BRIDGE_TOKEN`.
    pub fn from_env() -> Self {
        let project = std::env::var_os(crate::ENV_PROJECT)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        let server = Self::new(project, BridgeConfig::from_env());
        match std::env::var(crate::ENV_READ_ONLY).as_deref() {
            Ok("1") => server.read_only(),
            _ => server,
        }
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

/// Runs blocking filesystem/git work off the async executor, flattening
/// both failure kinds into the tool's error text.
async fn blocking<T: Send + 'static>(
    what: &str,
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("{what} failed: {e}"))?
        .map_err(err)
}

#[tool_router]
impl InfinaBoxServer {
    #[tool(
        description = "List every Context card in this game project (the markdown design \
notes in .ibproject/context/). Use this first to see what's already decided about the game."
    )]
    async fn list_context_cards(&self) -> Result<String, String> {
        let ctx = self.context()?;
        let overview = blocking("listing context cards", move || ctx.overview()).await?;
        if overview.is_empty() {
            return Ok("No Context cards yet.".to_string());
        }
        Ok(overview)
    }

    #[tool(
        description = "Read one Context card's full markdown. Use a path from \
list_context_cards or search_context."
    )]
    async fn read_context_card(
        &self,
        Parameters(CardPath { path }): Parameters<CardPath>,
    ) -> Result<String, String> {
        let ctx = self.context()?;
        blocking("reading a context card", move || ctx.read(&path)).await
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
        let ctx = self.context()?;
        let hits = blocking("searching context", move || ctx.search(&query)).await?;
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
        self.writable("saving a card")?;
        let ctx = self.context()?;
        let written = blocking("writing a context card", move || {
            ctx.write(&path, &markdown)
        })
        .await?;
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
        let snapshots = blocking("listing snapshots", move || {
            infinabox_core::snapshot::list_snapshots(&project, limit)
        })
        .await?;
        Ok(to_json(&snapshots))
    }

    #[tool(
        description = "A map of the game's structure: its name, main scene, autoloads \
(global singletons), input actions, every script (class name, what it extends, its \
signals and functions) and every scene (root script, node count). Read it before adding \
a system, so new code goes where the game's existing code is."
    )]
    async fn project_map(&self) -> Result<String, String> {
        let project = self.project()?.clone();
        let map = blocking("reading the game's structure", move || {
            infinabox_core::code_index::project_map(&project)
        })
        .await?;
        Ok(to_json(&map))
    }

    #[tool(
        description = "Find where a name is defined and every place it is used across the \
game's scripts and scenes: function calls, signal emits and connects (including the ones \
set up in scene files), and plain uses. Use it before renaming, removing or changing \
something, to see what else depends on it. Whole-word match, case-sensitive."
    )]
    async fn find_symbol(
        &self,
        Parameters(SymbolParams { name }): Parameters<SymbolParams>,
    ) -> Result<String, String> {
        let project = self.project()?.clone();
        let report = blocking("looking up a name", move || {
            infinabox_core::code_index::find_symbol(&project, &name)
        })
        .await?;
        Ok(to_json(&report))
    }

    #[tool(
        description = "Read one scene's structure without opening the raw .tscn file: its \
nodes (path, class, attached script, instanced scene, groups), signal connections, and the \
scripts and scenes it loads."
    )]
    async fn describe_scene(
        &self,
        Parameters(SceneParams { path }): Parameters<SceneParams>,
    ) -> Result<String, String> {
        let project = self.project()?.clone();
        let scene = blocking("reading a scene", move || {
            infinabox_core::code_index::describe_scene(&project, &path)
        })
        .await?;
        Ok(to_json(&scene))
    }

    /// Nothing is stored here: the agent runtime sees this call in the
    /// CLI's own stream and shows it as a plan card (`PlanProposed`). The
    /// checks are `infinabox_core::agent::prompt::validate_plan`, the same
    /// ones the stream parsers make, so every accepted plan becomes a card.
    #[tool(
        description = "Show the person a plan for a change to their game, as a card with \
Approve and Change buttons. Use it before changing the game (unless your instructions say \
this turn doesn't need a plan): read the relevant Context cards first, then give a short \
title and 2-6 plain-language steps (no code, no file paths unless they help). After calling \
it, end your turn with one short sentence and make no changes until the person approves."
    )]
    async fn propose_plan(
        &self,
        Parameters(PlanParams { title, steps }): Parameters<PlanParams>,
    ) -> Result<String, String> {
        self.writable("proposing a plan")?;
        infinabox_core::agent::prompt::validate_plan(&title, &steps)?;
        Ok(PLAN_SHOWN.to_string())
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
    async fn a_read_only_server_refuses_to_write() {
        let project = temp_project();
        let server = InfinaBoxServer::new(Some(project.clone()), None).read_only();
        let err = server
            .write_context_card(Parameters(WriteCard {
                path: "a.md".into(),
                markdown: "# A\n".into(),
            }))
            .await
            .unwrap_err();
        assert!(err.contains("question"), "{err}");
        assert!(!project.join(".ibproject/context/a.md").exists());
        let err = server
            .propose_plan(Parameters(PlanParams {
                title: "Do it".into(),
                steps: vec!["One".into()],
            }))
            .await
            .unwrap_err();
        assert!(err.contains("question"), "{err}");
        // Reading still works.
        assert_eq!(server.list_context_cards().await.unwrap(), "No Context cards yet.");
    }

    #[tokio::test]
    async fn the_index_tools_read_the_game() {
        let project = temp_project();
        std::fs::create_dir_all(project.join("scripts")).unwrap();
        std::fs::write(
            project.join("scripts/a.gd"),
            "extends Node\nsignal hit\nfunc go():\n\thit.emit()\n",
        )
        .unwrap();
        let server = InfinaBoxServer::new(Some(project), None);
        let map = server.project_map().await.unwrap();
        assert!(map.contains("scripts/a.gd"), "{map}");
        let found = server
            .find_symbol(Parameters(SymbolParams { name: "hit".into() }))
            .await
            .unwrap();
        assert!(found.contains("\"signal\"") && found.contains("\"emit\""), "{found}");
        assert!(server
            .find_symbol(Parameters(SymbolParams { name: "a b".into() }))
            .await
            .is_err());
        assert!(server
            .describe_scene(Parameters(SceneParams { path: "x.tscn".into() }))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn context_tools_work_against_a_temp_project() {
        let project = temp_project();
        let server = InfinaBoxServer::new(Some(project.clone()), None);

        assert_eq!(
            server.list_context_cards().await.unwrap(),
            "No Context cards yet."
        );
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

        server
            .write_context_card(Parameters(WriteCard {
                path: "tasks/fix.md".into(),
                markdown: "---\ntype: task\ntitle: Fix it\nstatus: doing\n---\nbody\n".into(),
            }))
            .await
            .unwrap();
        let bad = server
            .write_context_card(Parameters(WriteCard {
                path: "tasks/bad.md".into(),
                markdown: "---\ntype: task\nnot a pair\n---\n".into(),
            }))
            .await
            .unwrap_err();
        assert!(bad.contains("front-matter"), "{bad}");
        assert!(!project.join(".ibproject/context/tasks/bad.md").exists());
        assert_eq!(
            server.list_context_cards().await.unwrap(),
            "mechanics/double-jump.md  [other]  Double jump\n\
             tasks/fix.md  [task · doing]  Fix it"
        );

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

    fn plan(title: &str, steps: &[&str]) -> Parameters<PlanParams> {
        Parameters(PlanParams {
            title: title.into(),
            steps: steps.iter().map(|s| s.to_string()).collect(),
        })
    }

    #[tokio::test]
    async fn propose_plan_accepts_a_valid_plan_without_a_project_or_app() {
        let server = InfinaBoxServer::new(None, None);
        let text = server
            .propose_plan(plan(
                "Add a double jump",
                &[
                    "Let the player jump once more in the air.",
                    "Tune the height.",
                ],
            ))
            .await
            .unwrap();
        assert_eq!(text, PLAN_SHOWN);
        assert!(text.contains("Approve") && text.contains("End your turn now"));
    }

    #[tokio::test]
    async fn propose_plan_rejects_plans_outside_the_limits() {
        let server = InfinaBoxServer::new(None, None);
        let long_step = "x".repeat(201);
        let nine = ["s"; 9];
        for (title, steps, needle) in [
            (" ", &["A step."][..], "title"),
            (&"t".repeat(81)[..], &["A step."][..], "80"),
            ("Plan", &[][..], "at least one step"),
            ("Plan", &nine[..], "8"),
            ("Plan", &["Fine.", ""][..], "Step 2"),
            ("Plan", &[long_step.as_str()][..], "200"),
        ] {
            let e = server.propose_plan(plan(title, steps)).await.unwrap_err();
            assert!(e.contains(needle), "{e}");
        }
    }

    #[test]
    fn propose_plan_schema_has_title_and_steps() {
        let server = InfinaBoxServer::new(None, None);
        let tool = server
            .tool_router
            .list_all()
            .into_iter()
            .find(|t| t.name == "propose_plan")
            .unwrap();
        let schema = serde_json::to_value(&*tool.input_schema).unwrap();
        assert_eq!(schema["properties"]["title"]["type"], "string");
        assert_eq!(schema["properties"]["steps"]["type"], "array");
        assert_eq!(schema["properties"]["steps"]["items"]["type"], "string");
        let mut required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        required.sort();
        assert_eq!(required, ["steps", "title"]);
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
