use ggg::commands;

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
    Sync {
        /// Show what would be installed without writing any files
        #[arg(long)]
        dry_run: bool,
        /// Overwrite files even if they are not under ggg's control
        #[arg(long)]
        force: bool,
    },

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

    /// Add a new dependency
    ///
    /// Use `ggg add git <url>[@rev]` for git repositories or
    /// `ggg add archive <url>` for pre-built archives (.zip, .tar.gz, .tgz).
    ///
    /// Bare `ggg add <url>` auto-detects the type from the URL extension.
    #[command(subcommand)]
    Add(AddCommand),

    /// Remove a dependency
    Remove {
        /// Name of the dependency to remove
        name: String,
    },

    /// Show local changes to ggg-owned files
    ///
    /// Displays a unified diff between the version installed by ggg and the
    /// current on-disk content for every ggg-owned file that has been modified.
    /// Exits with code 1 when modified files are found, 0 when all files are
    /// unmodified.
    Diff {
        /// Show the diff for a specific file only
        file: Option<String>,
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

#[derive(Subcommand)]
enum AddCommand {
    /// Add a git repository dependency
    ///
    /// The URL may include an optional @rev suffix:
    ///   ggg add git https://github.com/user/repo.git@v1.0.0
    Git {
        /// Git URL, optionally with @rev suffix
        url: Option<String>,
        /// Accept all inferred defaults without prompting
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Add a pre-built archive dependency (.zip, .tar.gz, .tgz)
    Archive {
        /// Archive URL
        url: Option<String>,
        /// Dependency name (required; prompted if absent)
        #[arg(long)]
        name: Option<String>,
        /// Strip N leading path components from archive entries
        #[arg(long)]
        strip_components: Option<u32>,
        /// Expected SHA-256 hex digest; verified on download
        #[arg(long)]
        sha256: Option<String>,
    },

    /// Add a dependency, auto-detecting type from the URL
    ///
    /// Routes to `git` if the URL looks like a git remote, `archive` if it
    /// ends in a recognised archive extension, or prompts if ambiguous.
    #[command(external_subcommand)]
    Url(Vec<String>),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init                      => commands::init::run(),
        Command::Sync { dry_run, force }   => commands::sync::run(dry_run, force),
        Command::Edit { args }             => commands::edit::run(&args),
        Command::Run { args }              => commands::run::run(&args),
        Command::Add(add_cmd) => match add_cmd {
            AddCommand::Git { url, yes } =>
                commands::add::run_git(url.as_deref(), yes),
            AddCommand::Archive { url, name, strip_components, sha256 } =>
                commands::add::run_archive(url.as_deref(), name.as_deref(), strip_components, sha256.as_deref()),
            AddCommand::Url(args) => match args.first().map(String::as_str) {
                Some(url) => commands::add::run_bare(url, false),
                None      => commands::add::run_git(None, false),
            },
        },
        Command::Remove { name }           => commands::remove::run(&name),
        Command::Diff { file }             => commands::diff::run(file.as_deref()),
        Command::SelfUpdate { command: _ } => anyhow::bail!("not yet implemented"),
    }
}
