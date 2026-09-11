mod watcher;
mod git_indexer;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "infinabox-core")]
#[command(about = "Phase 0 spike: prove the file watcher and Git indexer stay accurate on a real project")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Watch a project directory and print structured file events as they happen
    Watch {
        /// Path to the project folder to watch
        #[arg(long)]
        path: String,
    },
    /// Walk a Git repository's full commit history and print it as structured JSON
    Index {
        /// Path to the Git repository to index
        #[arg(long)]
        path: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Watch { path } => watcher::run(&path),
        Command::Index { path } => git_indexer::run(&path),
    }
}
