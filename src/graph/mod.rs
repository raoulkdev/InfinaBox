//! Milestone 4 (Phase 0): the one local map — a SQLite-backed node/edge graph
//! combining the Git indexer's commit history with a persistent file identity
//! that survives renames.
//!
//! This is the single most important module in the spike. Everything else in
//! the product plan (agents, panels, the "trace a bug to its cause" move) is
//! just a different way of querying this map. If identity doesn't survive a
//! rename here, every agent built on top of it will confidently be wrong.

pub mod ingest;
pub mod query;
pub mod schema;

pub use schema::open_and_init;
