mod commands;
use ggg::config;
use ggg::dependency;
use ggg::godot;

use anyhow::Result;
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
    ///
    /// All arguments after `edit` are forwarded verbatim to Godot.
    /// ggg-level flags (e.g. --no-download) must come before the subcommand.
    Edit {
        /// Arguments forwarded verbatim to the Godot editor
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run Godot against the project
    ///
    /// All arguments after `run` are forwarded verbatim to Godot.
    /// ggg-level flags (e.g. --no-download) must come before the subcommand.
    Run {
        /// Arguments forwarded verbatim to Godot
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init                      => commands::init::run(),
        Command::Sync                      => commands::sync::run(),
        Command::Edit { args }             => commands::edit::run(&args),
        Command::Run { args }              => commands::run::run(&args),
        Command::Add { git_url }           => commands::add::run(&git_url),
        Command::Remove { name }           => commands::remove::run(&name),
        Command::SelfUpdate { command: _ } => anyhow::bail!("not yet implemented"),
    }
}
