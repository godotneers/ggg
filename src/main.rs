mod config;
mod godot;

use clap::{Parser, Subcommand};

/// A project manager for Godot games.
///
/// Pins a specific Godot version per project, manages addon dependencies
/// from git sources, and provides a unified CLI to sync, edit, and run
/// your project.
#[derive(Parser)]
#[command(name = "ggg", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a ggg.toml in the current directory
    Init,

    /// Resolve and install all dependencies, download Godot if needed
    Sync,

    /// Open the project in the pinned Godot editor
    Edit,

    /// Run Godot against the project
    ///
    /// Pass additional arguments after `--`, e.g. `ggg run -- --headless`
    Run {
        /// Extra arguments forwarded to Godot
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Add a new dependency interactively
    Add {
        /// Git URL of the addon repository
        git_url: String,
    },

    /// Remove a dependency
    Remove {
        /// Name of the dependency to remove
        name: String,
    },

    /// Update Godot Goodie Grabber itself
    #[command(name = "self")]
    SelfUpdate {
        #[command(subcommand)]
        command: SelfCommand,
    },
}

#[derive(Subcommand)]
enum SelfCommand {
    /// Download and install the latest release
    Update,
}

fn main() {
    let _cli = Cli::parse();
}
