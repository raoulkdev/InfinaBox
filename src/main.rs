mod ask;
mod git_indexer;
mod graph;
mod mcp_client;
mod watcher;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "infinabox-core")]
#[command(about = "Phase 0 spike: prove the shared project memory holds up on a real repo")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Watch a project directory and print structured file events as they happen
    Watch {
        #[arg(long)]
        path: String,
    },
    /// Walk a Git repository's full commit history and print it as structured JSON
    Index {
        #[arg(long)]
        path: String,
    },
    /// Build (or refresh) the local project graph from a repo's Git history
    BuildGraph {
        /// Path to the Git repository to ingest
        #[arg(long)]
        repo: String,
        /// Path to the SQLite graph database file (created if missing)
        #[arg(long)]
        db: String,
    },
    /// Query the local project graph
    Query {
        #[command(subcommand)]
        what: QueryCommand,
    },
    /// Ask a grounded question, answered only from what's in the project graph
    Ask {
        #[arg(long)]
        db: String,
        /// The question, e.g. "why did ElevatorLogic.gd break"
        question: String,
    },
    /// Round-trip a request to a Godot MCP server (mock, until Godot + a real
    /// MCP plugin is installed) and print what comes back
    GodotPing {
        /// Path to the MCP server executable to spawn and talk to
        #[arg(long, default_value = "target/debug/mock_godot_mcp")]
        cmd: String,
    },
}

#[derive(Subcommand)]
enum QueryCommand {
    /// What files changed in a given commit
    Commit {
        #[arg(long)]
        db: String,
        #[arg(long)]
        sha: String,
    },
    /// What commits ever touched a given file path (survives renames)
    File {
        #[arg(long)]
        db: String,
        #[arg(long)]
        path: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Watch { path } => watcher::run(&path),
        Command::Index { path } => git_indexer::run(&path),
        Command::BuildGraph { repo, db } => {
            let report = graph::ingest::build_graph(&repo, &db)?;
            println!("{report:#?}");
            Ok(())
        }
        Command::Query { what } => match what {
            QueryCommand::Commit { db, sha } => {
                let conn = graph::open_and_init(&db)?;
                let results = graph::query::files_changed_in_commit(&conn, &sha)?;
                println!("{}", serde_json::to_string_pretty(&results)?);
                Ok(())
            }
            QueryCommand::File { db, path } => {
                let conn = graph::open_and_init(&db)?;
                let results = graph::query::commits_touching_file(&conn, &path)?;
                println!("{}", serde_json::to_string_pretty(&results)?);
                Ok(())
            }
        },
        Command::Ask { db, question } => {
            let conn = graph::open_and_init(&db)?;
            let answer = ask::answer(&conn, &question)?;
            println!("{answer}");
            Ok(())
        }
        Command::GodotPing { cmd } => mcp_client::ping(&cmd),
    }
}
