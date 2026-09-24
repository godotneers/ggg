use ggg::commands;

use anyhow::{Result, bail};
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
    Init {
        /// Download and install export templates alongside the Godot version
        #[arg(long)]
        with_export_templates: bool,
    },

    /// Resolve and install all dependencies, download Godot if needed
    Sync {
        /// Use this Godot executable instead of the managed one
        #[arg(long)]
        godot: Option<String>,
        /// Show what would be installed without writing any files
        #[arg(long)]
        dry_run: bool,
        /// Overwrite files even if they are not under ggg's control
        #[arg(long)]
        force: bool,
        /// Download and install export templates alongside the Godot version
        #[arg(long)]
        with_export_templates: bool,
    },

    /// Open the project in the pinned Godot editor
    ///
    /// All arguments after `edit` are forwarded verbatim to Godot.
    /// ggg-level flags must come before the subcommand.
    Edit {
        /// Use this Godot executable instead of the managed one
        #[arg(long)]
        godot: Option<String>,
        /// Download and install export templates alongside the Godot version
        #[arg(long)]
        with_export_templates: bool,
        /// Arguments forwarded verbatim to the Godot editor
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run Godot against the project
    ///
    /// All arguments after `run` are forwarded verbatim to Godot.
    /// ggg-level flags must come before the subcommand.
    Run {
        /// Use this Godot executable instead of the managed one
        #[arg(long)]
        godot: Option<String>,
        /// Download and install export templates alongside the Godot version
        #[arg(long)]
        with_export_templates: bool,
        /// Arguments forwarded verbatim to Godot
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Add a new dependency
    ///
    /// The type keyword (git, archive, asset-store, asset-library, asset) is
    /// optional; ggg infers it when omitted:
    ///   - archive extensions (.zip, .tar.gz, .tgz) -> archive
    ///   - Asset Store references (publisher/slug[:version]) -> asset-store
    ///   - git-style URLs (containing ://, ending in .git, or SCP-style) -> git
    ///   - numeric IDs -> asset-library
    ///   - anything else -> search of the Asset Store and Asset Library
    ///
    /// `asset` is an alias for `asset-store`. Asset Store dependencies always
    /// pin a version; a bare publisher/slug resolves to the latest stable
    /// release compatible with the project's Godot version (add `:version` to
    /// pin an exact one).
    ///
    /// Examples:
    ///   ggg add https://github.com/user/addon.git@v1.0
    ///   ggg add git https://github.com/user/addon.git@v1.0
    ///   ggg add archive https://example.com/addon.zip --sha256 <hash>
    ///   ggg add asset-store souleat/godot-xoshiro256-plus-plus
    ///   ggg add asset souleat/godot-xoshiro256-plus-plus:1.1.0
    ///   ggg add asset-store decal-co --yes
    ///   ggg add asset-library --id 54
    ///   ggg add 54
    ///   ggg add gut
    #[command(verbatim_doc_comment)]
    Add(AddArgs),

    /// Search the Godot Asset Store or Asset Library for addons
    ///
    /// Filters results to addons compatible with the Godot version declared in
    /// ggg.toml.  The default source is the Asset Store; pass `--source
    /// asset-library` to search the legacy Asset Library instead.
    Search {
        /// Search query
        query: String,
        /// Override the Godot version used for filtering (e.g. "4.3")
        #[arg(long)]
        godot_version: Option<String>,
        /// Source to search: asset-store (default) or asset-library
        #[arg(long, value_enum, default_value_t = commands::search::SearchSource::AssetStore)]
        source: commands::search::SearchSource,
    },

    /// Check for updates to Godot Asset Library and Asset Store dependencies
    ///
    /// Queries the asset library / asset store for the current version of each
    /// dep and, if a newer one is available, drops the lock entry so that
    /// `ggg sync` fetches it. Store deps also get their pinned version in
    /// ggg.toml bumped to the newest compatible release.  Omit the name to
    /// check all eligible dependencies.
    ///
    /// Only works for dependencies added via `ggg add asset-library` or `ggg
    /// add asset-store`.  For git or archive dependencies, update by editing
    /// ggg.toml and running ggg sync.
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
    /// Dependency type (git, archive, asset-library, asset-store, asset) or
    /// URL/query for auto-detection
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

    /// Use this asset ID directly, skipping the search (asset-library only)
    #[arg(long, help_heading = "Asset Library Options")]
    id: Option<u32>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init {
            with_export_templates,
        } => commands::init::run(with_export_templates),
        Command::Sync {
            godot,
            dry_run,
            force,
            with_export_templates,
        } => commands::sync::run(dry_run, force, with_export_templates, godot),
        Command::Edit {
            godot,
            with_export_templates,
            args,
        } => commands::edit::run(&args, with_export_templates, godot),
        Command::Run {
            godot,
            with_export_templates,
            args,
        } => commands::run::run(&args, with_export_templates, godot),
        Command::Add(AddArgs {
            type_or_input,
            input,
            name,
            yes,
            strip_components,
            sha256,
            id,
        }) => {
            let name = name.as_deref();
            let sha256 = sha256.as_deref();
            let id_invalid = || bail!("--id is only valid with `ggg add asset-library`");
            let sha_invalid = || bail!("--sha256 is only valid with `ggg add archive`");
            match type_or_input.as_deref() {
                Some("git") => {
                    if id.is_some() {
                        id_invalid()?;
                    }
                    if sha256.is_some() {
                        sha_invalid()?;
                    }
                    if strip_components.is_some() {
                        bail!("--strip-components is not valid for git dependencies");
                    }
                    commands::add::run_git(input.as_deref(), name, yes)
                }
                Some("archive") => {
                    if id.is_some() {
                        id_invalid()?;
                    }
                    commands::add::run_archive(
                        input.as_deref(),
                        name,
                        strip_components,
                        sha256,
                        yes,
                    )
                }
                Some("asset-library") => {
                    if sha256.is_some() {
                        sha_invalid()?;
                    }
                    commands::add::run_asset(input.as_deref(), id, name, yes, strip_components)
                }
                Some("asset" | "asset-store") => {
                    if id.is_some() {
                        bail!(
                            "--id is only valid with `ggg add asset-library`; \
                             Asset Store references use publisher/slug[:version]"
                        );
                    }
                    if sha256.is_some() {
                        sha_invalid()?;
                    }
                    commands::add::run_asset_store(input.as_deref(), name, yes, strip_components)
                }
                Some(url_or_query) => {
                    commands::add::run_bare(url_or_query, name, yes, strip_components, sha256, id)
                }
                None => bail!(
                    "specify a type (git, archive, asset-store, asset-library, asset) \
                     or provide a URL/query"
                ),
            }
        }
        Command::Deps => commands::deps::run(),
        Command::Remove { name } => commands::remove::run(&name),
        Command::Diff { file } => commands::diff::run(file.as_deref()),
        Command::LsDep { name, all } => commands::ls_dep::run(&name, all),
        Command::Search {
            query,
            godot_version,
            source,
        } => commands::search::run(&query, godot_version.as_deref(), source),
        Command::Update { name, dry_run } => commands::update::run(name.as_deref(), dry_run),
    }
}
