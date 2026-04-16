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
    /// Use `ggg add git <url>[@rev]` for git repositories,
    /// `ggg add archive <url>` for pre-built archives (.zip, .tar.gz, .tgz),
    /// or `ggg add asset <name|id>` to search the Godot Asset Library.
    ///
    /// Bare `ggg add <input>` auto-detects the type: archive extensions route
    /// to archive, git URLs route to git, and anything else searches the asset
    /// library.
    #[command(subcommand)]
    Add(AddCommand),

    /// Search the Godot Asset Library
    ///
    /// Filters results to assets compatible with the Godot version declared in
    /// ggg.toml.  Use `ggg add asset --id <N>` to add a specific result.
    Search {
        /// Search query
        query: String,
        /// Override the Godot version used for filtering (e.g. "4.3")
        #[arg(long)]
        godot_version: Option<String>,
    },

    /// Check for updates to Godot Asset Library dependencies
    ///
    /// Queries the asset library for the current version of each dep and, if
    /// a newer one is available, drops the lock entry so that `ggg sync`
    /// fetches it.  Omit the name to check all asset library dependencies.
    ///
    /// Only works for dependencies added via `ggg add asset`.  For git or
    /// archive dependencies, update by editing ggg.toml and running ggg sync.
    Update {
        /// Name of the dependency to check (omit to check all)
        name: Option<String>,
        /// Show available updates without modifying ggg.lock
        #[arg(long)]
        dry_run: bool,
    },

    /// Remove a dependency
    Remove {
        /// Name of the dependency to remove
        name: String,
    },

    /// List all dependencies declared in ggg.toml
    Deps,

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

    /// List the raw contents of a dependency's cache entry
    ///
    /// Shows the source tree before strip_components or map are applied, so
    /// you can determine the right values before running ggg sync.
    ///
    /// If the dependency is not yet cached it is fetched and the lock file is
    /// updated as a side effect.
    LsDep {
        /// Name of the dependency (must be present in ggg.toml)
        name: String,
        /// Show every file path individually instead of a collapsed tree
        #[arg(long)]
        all: bool,
    },
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

    /// Add a Godot Asset Library dependency
    ///
    /// Searches the asset library for the given name or resolves the given
    /// numeric asset ID directly.  The Godot version from ggg.toml is used
    /// to filter search results.
    ///
    /// Examples:
    ///   ggg add asset gut
    ///   ggg add asset --id 54
    Asset {
        /// Asset name to search for, or numeric asset ID
        query: Option<String>,
        /// Use this asset ID directly, skipping the search
        #[arg(long)]
        id: Option<u32>,
        /// Accept inferred defaults without prompting
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Add a dependency, auto-detecting type from the URL or search term
    ///
    /// Routes to `git` if the input looks like a git remote, `archive` if it
    /// ends in a recognised archive extension, and `asset` (Godot Asset
    /// Library search) for anything else.
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
            AddCommand::Asset { query, id, yes } =>
                commands::add::run_asset(query.as_deref(), id, yes),
            AddCommand::Url(args) => match args.first().map(String::as_str) {
                Some(input) => commands::add::run_bare(input, false),
                None        => commands::add::run_git(None, false),
            },
        },
        Command::Deps                      => commands::deps::run(),
        Command::Remove { name }           => commands::remove::run(&name),
        Command::Diff { file }             => commands::diff::run(file.as_deref()),
        Command::LsDep { name, all }       => commands::ls_dep::run(&name, all),
        Command::Search { query, godot_version } =>
            commands::search::run(&query, godot_version.as_deref()),
        Command::Update { name, dry_run } =>
            commands::update::run(name.as_deref(), dry_run),
    }
}
