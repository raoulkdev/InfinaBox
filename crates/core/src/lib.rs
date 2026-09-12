//! InfinaBox's shared project memory: the file watcher, Git indexer, local
//! graph, grounded question-answering, and MCP client — reusable by both the
//! `infinabox-cli` spike tool and the Tauri desktop app.

pub mod ask;
pub mod git_indexer;
pub mod graph;
pub mod mcp_client;
pub mod watcher;
