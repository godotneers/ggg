use ggg::commands;

use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand};

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
    /// The type keyword (git, archive, asset) is optional; ggg infers it when omitted:
    ///   - archive extensions (.zip, .tar.gz, .tgz) -> archive
    ///   - git-style URLs (containing ://, ending in .git, or SCP-style) -> git
    ///   - anything else -> Godot Asset Library search
    ///
    /// Examples:
    ///   ggg add https://github.com/user/addon.git@v1.0
    ///   ggg add git https://github.com/user/addon.git@v1.0
    ///   ggg add archive https://example.com/addon.zip --sha256 <hash>
    ///   ggg add asset gut
    ///   ggg add asset --id 54
    #[command(verbatim_doc_comment)]
    Add(AddArgs),

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

#[derive(Args)]
struct AddArgs {
    /// Dependency type (git, archive, asset) or URL/query for auto-detection
    #[arg(value_name = "TYPE_OR_INPUT")]
    type_or_input: Option<String>,

    /// URL or search query (when an explicit type keyword is given as first argument)
    #[arg(value_name = "INPUT")]
    input: Option<String>,

    /// Dependency name (overrides the name inferred from the URL or asset title)
    #[arg(long)]
    name: Option<String>,

    /// Accept all inferred defaults without prompting
    #[arg(long, short = 'y')]
    yes: bool,

    /// Strip N leading path components from archive or asset entries
    #[arg(long)]
    strip_components: Option<u32>,

    /// Expected SHA-256 hex digest; verified on download (archive only)
    #[arg(long, help_heading = "Archive Options")]
    sha256: Option<String>,

    /// Use this asset ID directly, skipping the search (asset only)
    #[arg(long, help_heading = "Asset Library Options")]
    id: Option<u32>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init                      => commands::init::run(),
        Command::Sync { dry_run, force }   => commands::sync::run(dry_run, force),
        Command::Edit { args }             => commands::edit::run(&args),
        Command::Run { args }              => commands::run::run(&args),
        Command::Add(AddArgs { type_or_input, input, name, yes, strip_components, sha256, id }) => {
            let name = name.as_deref();
            match type_or_input.as_deref() {
                Some("git")        => commands::add::run_git(input.as_deref(), name, yes),
                Some("archive")    => commands::add::run_archive(input.as_deref(), name, strip_components, sha256.as_deref()),
                Some("asset")      => commands::add::run_asset(input.as_deref(), id, name, yes),
                Some(url_or_query) => commands::add::run_bare(url_or_query, name, yes),
                None               => bail!("specify a type (git, archive, asset) or provide a URL/query"),
            }
        }
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
