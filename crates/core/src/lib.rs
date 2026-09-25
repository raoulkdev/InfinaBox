//! InfinaBox's shared project memory: the file watcher, Git indexer, local
//! graph, grounded question-answering, and MCP client — reusable by both the
//! `infinabox-cli` spike tool and the Tauri desktop app. Phase A adds the
//! agent runtime, Godot integration, snapshots, chat storage, and project
//! scaffolding.

pub mod agent;
pub mod ask;
pub mod chat_store;
pub mod git_indexer;
pub mod godot;
pub mod graph;
pub mod mcp_client;
pub mod redact;
pub mod scaffold;
pub mod snapshot;
pub mod watcher;
