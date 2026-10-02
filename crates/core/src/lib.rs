//! InfinaBox's shared project memory: the file watcher, Git indexer, local
//! graph, grounded question-answering, and MCP client — reusable by both the
//! `infinabox-cli` spike tool and the Tauri desktop app. Phase A adds the
//! agent runtime, Godot integration, snapshots, chat storage, and project
//! scaffolding; Phase B adds connecting the user's AI, settings, the
//! onboarding interview, and the automatic error-fix loop.

pub mod agent;
pub mod app_settings;
pub mod assets;
pub mod ask;
pub mod autofix;
pub mod boards;
pub mod chat_store;
pub mod connect;
pub mod context_cards;
pub mod generate;
pub mod git_indexer;
pub mod godot;
pub mod graph;
pub mod library;
pub mod mcp_client;
pub mod onboarding;
pub mod producer;
pub mod project_settings;
pub mod redact;
pub mod scaffold;
pub mod secrets;
pub mod skills;
pub mod snapshot;
pub mod watcher;
