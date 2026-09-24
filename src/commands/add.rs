//! Implementation of `ggg add`.
//!
//! Four subcommands:
//!
//! - `ggg add git <url>[@rev]` - adds a git dependency, resolves the rev
//!   against the remote before writing.
//! - `ggg add archive <url>` - adds an archive dependency; no network call at
//!   add time (the sha256 field, if provided, is verified on first sync).
//! - `ggg add asset-library <query|id> [--id N]` - searches the Godot Asset
//!   Library and adds the matching dep by asset ID.
//! - `ggg add asset-store <publisher/slug[:version]>` (alias `asset`) - adds a
//!   Godot Asset Store dependency. The version is always pinned; a bare
//!   `publisher/slug` resolves to the latest stable release compatible with the
//!   project's Godot version at add time.
//!
//! The bare `ggg add <input>` form infers the type: archive URLs route to the
//! archive path, store references (`publisher/slug[:version]`) route to the
//! store, git URLs route to git, numeric IDs to the asset library, and
//! anything else becomes a combined store + library search.

use std::path::Path;

use anyhow::{Context, Result, bail};
use dialoguer::{Input, Select, theme::ColorfulTheme};
use indicatif::ProgressBar;

use crate::config::{AssetStoreRef, Config, Dependency, Source};
use crate::dependency::resolver;
use crate::godot::asset_lib;
use crate::godot::asset_lib::AssetSearchResult;
use crate::godot::asset_store;
use crate::godot::asset_store::{StoreAsset, StoreRelease};
use crate::godot::release::GodotVersion;
use crate::utils::validation::{validate_asset_store_slug, validate_version_tag};

pub fn run_git(git_url: Option<&str>, name_arg: Option<&str>, yes: bool) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;
    let theme = ColorfulTheme::default();

    let (url_arg, rev_arg) = match git_url {
        Some(s) => {
            let (url, rev) = parse_url_rev(s);
            (Some(url), rev)
        }
        None => (None, None),
    };

    if yes && url_arg.is_none() {
        bail!("--yes requires a git URL argument");
    }

    let git = match url_arg {
        Some(u) => u,
        None => Input::with_theme(&theme)
            .with_prompt("Git URL")
            .interact_text()?,
    };

    if yes && rev_arg.is_none() {
        bail!("--yes requires a revision; append it to the URL with @rev");
    }

    let rev = match rev_arg {
        Some(r) => r,
        None => Input::with_theme(&theme)
            .with_prompt("Revision (branch, tag, or commit SHA)")
            .default("main".to_owned())
            .interact_text()?,
    };

    let default_name = infer_name_from_git(&git);
    let name = resolve_name(name_arg, Some(default_name), yes, &theme)?;

    if config.dependency.iter().any(|d| d.name == name) {
        bail!("a dependency named {:?} already exists in ggg.toml", name);
    }

    let dep = Dependency::new(
        &name,
        Source::Git {
            git: git.clone(),
            rev: rev.clone(),
        },
        None,
        None,
    );
    let spinner = ProgressBar::new_spinner();
    spinner.set_message(format!("Resolving {rev}..."));
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    let resolved = resolver::resolve(&dep)
        .with_context(|| format!("failed to resolve {:?} from {git:?}", rev))?;
    spinner.finish_and_clear();

    config.dependency.push(dep);
    config.save(ggg_toml)?;

    println!(
        "Added {name:?} ({rev}) resolved to {}",
        &resolved.sha()[..12]
    );
    println!("Run `ggg sync` to install it.");
    Ok(())
}

pub fn run_archive(
    archive_url: Option<&str>,
    name_arg: Option<&str>,
    strip_components: Option<u32>,
    sha256: Option<&str>,
    yes: bool,
) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;
    let theme = ColorfulTheme::default();

    let url = match archive_url {
        Some(u) => u.to_owned(),
        None => Input::with_theme(&theme)
            .with_prompt("Archive URL (.zip, .tar.gz, .tgz)")
            .interact_text()?,
    };

    // Validate the URL extension up front.
    let supported = url.ends_with(".zip") || url.ends_with(".tar.gz") || url.ends_with(".tgz");
    if !supported {
        bail!("unrecognised archive format in URL {url:?}; supported: .zip, .tar.gz, .tgz");
    }

    // Default name: derive it from the URL's filename so `--yes` works without
    // `--name` ("debug_draw_3d.zip" -> "debug-draw-3d").
    let default_name = infer_name_from_archive_url(&url);
    let name = resolve_name(name_arg, Some(default_name), yes, &theme)?;
    if name.is_empty() {
        bail!("dependency name cannot be empty");
    }

    if config.dependency.iter().any(|d| d.name == name) {
        bail!("a dependency named {:?} already exists in ggg.toml", name);
    }

    let dep = Dependency::new(
        &name,
        Source::Archive {
            url: url.clone(),
            sha256: sha256.map(str::to_owned),
            strip_components,
        },
        None,
        None,
    );

    config.dependency.push(dep);
    config.save(ggg_toml)?;

    println!("Added {name:?} from {url}");
    if sha256.is_none() {
        println!("Tip: add a sha256 = \"<hash>\" field to verify the download integrity.");
    }
    println!("Run `ggg sync` to install it.");
    Ok(())
}

/// Handle `ggg add <input>` with no explicit subcommand.
///
/// - Archive extensions (`.zip`, `.tar.gz`, `.tgz`) -> archive dep.
/// - Asset Store references (`publisher/slug[:version]`) -> asset-store.
/// - URL-like strings (`://`, `.git`, SCP-style) -> git dep.
/// - A pure number -> asset library by ID.
/// - Anything else (plain query) -> combined store + library search.
pub fn run_bare(
    input: &str,
    name_arg: Option<&str>,
    yes: bool,
    strip_components: Option<u32>,
    sha256: Option<&str>,
    id_override: Option<u32>,
) -> Result<()> {
    if id_override.is_some() {
        bail!("--id is only valid with `ggg add asset-library`");
    }
    let sha_invalid = || bail!("--sha256 is only valid with `ggg add archive`");
    if input.ends_with(".zip") || input.ends_with(".tar.gz") || input.ends_with(".tgz") {
        run_archive(Some(input), name_arg, strip_components, sha256, yes)
    } else if looks_like_store_spec(input) {
        if sha256.is_some() {
            sha_invalid()?;
        }
        run_asset_store(Some(input), name_arg, yes, strip_components)
    } else if input.contains("://") || input.ends_with(".git") || input.contains(':') {
        if sha256.is_some() {
            sha_invalid()?;
        }
        if strip_components.is_some() {
            bail!("--strip-components is not valid for git dependencies");
        }
        run_git(Some(input), name_arg, yes)
    } else if input.trim().parse::<u32>().is_ok() {
        if sha256.is_some() {
            sha_invalid()?;
        }
        run_asset(Some(input), None, name_arg, yes, strip_components)
    } else {
        if sha256.is_some() {
            sha_invalid()?;
        }
        run_combined(input, name_arg, yes, strip_components)
    }
}

/// Handle `ggg add asset-store [<publisher/slug[:version]|query>]`.
///
/// A spec (`publisher/slug[:version]`) is resolved against the Asset Store
/// API. The version is always pinned for the config: a bare spec picks the
/// largest stable release compatible with the project's Godot version, and a
/// pinned one (via the `:version` suffix) is used as-is (with a warning when
/// it is incompatible instead of an error).
///
/// A non-spec query searches the store using the unified search/disambiguation
/// flow. `asset` is an alias for this command.
pub fn run_asset_store(
    spec_arg: Option<&str>,
    name_arg: Option<&str>,
    yes: bool,
    strip_components: Option<u32>,
) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;
    let theme = ColorfulTheme::default();
    let godot_version: GodotVersion = config.project.godot.version.clone();
    let version_label = godot_version.major_minor();

    if yes && spec_arg.is_none() {
        bail!("--yes requires an asset reference (publisher/slug[:version])");
    }

    let raw = match spec_arg {
        Some(s) => s.to_owned(),
        None => Input::with_theme(&theme)
            .with_prompt("Asset Store asset (publisher/slug[:version] or keyword)")
            .interact_text()?,
    };

    // A pure number can never be an Asset Store reference.
    if let Ok(id) = raw.trim().parse::<u32>() {
        bail!(
            "Asset Store references use publisher/slug[:version], not numeric IDs; \
             for the Asset Library use `ggg add asset-library --id {id}`"
        );
    }

    match parse_store_input(&raw)? {
        Some(spec) => {
            let asset =
                asset_store::get_asset(&spec.publisher, &spec.asset).with_context(|| {
                    format!(
                        "failed to fetch {}/{} from the Godot Asset Store",
                        spec.publisher, spec.asset
                    )
                })?;
            let release = resolve_store_release(
                &spec.publisher,
                &spec.asset,
                spec.pinned.as_deref(),
                &godot_version,
            )?;
            add_store_asset(
                &mut config,
                &spec.publisher,
                &spec.asset,
                &asset,
                &release,
                &godot_version,
                name_arg,
                yes,
                strip_components,
                &theme,
            )
        }
        None => {
            let (results, total) = asset_store::search(&raw, Some(&godot_version))
                .context("failed to search the Godot Asset Store")?;
            let hits = results.into_iter().map(SearchResult::Store).collect();
            let tip = "then `ggg add asset-store <publisher>/<slug>` to add a specific one";
            let SearchResult::Store(asset) =
                disambiguate(hits, total, &raw, &version_label, tip, &theme)?
            else {
                unreachable!("store search cannot return Asset Library hits");
            };
            let release =
                resolve_store_release(&asset.publisher.slug, &asset.slug, None, &godot_version)?;
            add_store_asset(
                &mut config,
                &asset.publisher.slug,
                &asset.slug,
                &asset,
                &release,
                &godot_version,
                name_arg,
                yes,
                strip_components,
                &theme,
            )
        }
    }
}

/// Handle `ggg add asset-library [<query|id>] [--id N]`.
///
/// If `id_override` is given the asset is fetched directly.  Otherwise
/// `query` is used as a search term; a pure number is treated as an ID.
///
/// A query search uses the unified search/disambiguation flow (0 -> error,
/// 1 -> auto-add, 2-5 -> picker, 6+ -> error suggesting `ggg search`).
pub fn run_asset(
    query: Option<&str>,
    id_override: Option<u32>,
    name_arg: Option<&str>,
    yes: bool,
    strip_components: Option<u32>,
) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;
    let theme = ColorfulTheme::default();

    let godot_version: GodotVersion = config.project.godot.version.clone();
    let version_label = godot_version.major_minor();

    // Resolve to a single AssetDetail.
    let detail = if let Some(id) = id_override {
        asset_lib::get_asset(id).with_context(|| {
            format!("failed to fetch asset id {id} from the Godot Asset Library")
        })?
    } else {
        let q = match query {
            Some(q) => q.to_owned(),
            None => Input::with_theme(&theme)
                .with_prompt("Asset name or ID")
                .interact_text()?,
        };

        // A pure number is treated as a direct asset ID.
        if let Ok(id) = q.trim().parse::<u32>() {
            asset_lib::get_asset(id).with_context(|| {
                format!("failed to fetch asset id {id} from the Godot Asset Library")
            })?
        } else {
            let (results, total) = asset_lib::search(&q, Some(&godot_version))
                .context("failed to search the Godot Asset Library")?;
            let hits = results.into_iter().map(SearchResult::Library).collect();
            let tip = "then `ggg add asset-library --id <N>` to add a specific one";
            match disambiguate(hits, total, &q, &version_label, tip, &theme)? {
                SearchResult::Library(result) => asset_lib::get_asset(result.asset_id)
                    .with_context(|| {
                        "failed to fetch asset details from the Godot Asset Library".to_string()
                    })?,
                SearchResult::Store(_) => {
                    unreachable!("library search cannot return Asset Store hits")
                }
            }
        }
    };

    add_asset_lib_dep(&mut config, detail, name_arg, yes, strip_components, &theme)
}

/// Handle a bare plain-query add: search the Asset Store and Asset Library
/// together and let the unified disambiguation flow pick one.
fn run_combined(
    query: &str,
    name_arg: Option<&str>,
    yes: bool,
    strip_components: Option<u32>,
) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;
    let theme = ColorfulTheme::default();
    let godot_version: GodotVersion = config.project.godot.version.clone();
    let version_label = godot_version.major_minor();

    let (store_results, store_total) = asset_store::search(query, Some(&godot_version))
        .context("failed to search the Godot Asset Store")?;
    let (lib_results, lib_total) = asset_lib::search(query, Some(&godot_version))
        .context("failed to search the Godot Asset Library")?;

    let mut hits = Vec::with_capacity(store_results.len() + lib_results.len());
    hits.extend(store_results.into_iter().map(SearchResult::Store));
    hits.extend(lib_results.into_iter().map(SearchResult::Library));
    let total = store_total + lib_total;
    let tip = "then add the one you want with `ggg add asset-store <publisher>/<slug>` \
               or `ggg add asset-library --id <N>`";

    match disambiguate(hits, total, query, &version_label, tip, &theme)? {
        SearchResult::Store(asset) => {
            let release =
                resolve_store_release(&asset.publisher.slug, &asset.slug, None, &godot_version)?;
            add_store_asset(
                &mut config,
                &asset.publisher.slug,
                &asset.slug,
                &asset,
                &release,
                &godot_version,
                name_arg,
                yes,
                strip_components,
                &theme,
            )
        }
        SearchResult::Library(result) => {
            let detail = asset_lib::get_asset(result.asset_id).with_context(|| {
                "failed to fetch asset details from the Godot Asset Library".to_string()
            })?;
            add_asset_lib_dep(&mut config, detail, name_arg, yes, strip_components, &theme)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_name(
    name_arg: Option<&str>,
    default: Option<String>,
    yes: bool,
    theme: &ColorfulTheme,
) -> Result<String> {
    if let Some(n) = name_arg {
        return Ok(n.to_owned());
    }
    if yes {
        return Ok(default.unwrap_or_default());
    }
    let mut b = Input::with_theme(theme).with_prompt("Name");
    if let Some(d) = default {
        b = b.default(d);
    }
    Ok(b.interact_text()?)
}

/// Split `s` into a git URL and an optional revision.
fn parse_url_rev(s: &str) -> (String, Option<String>) {
    if let Some((left, right)) = s.rsplit_once('@') {
        let looks_like_url = left.contains("://") || left.ends_with(".git") || left.contains(':');
        if looks_like_url {
            return (left.to_owned(), Some(right.to_owned()));
        }
    }
    (s.to_owned(), None)
}

/// Derive a dependency name from a Godot Asset Library asset title.
///
/// Takes everything before the first  " - " separator (if any), lowercases it,
/// replaces non-alphanumeric characters with hyphens, and collapses runs.
///
/// Examples: "GUT - Godot Unit Testing" -> "gut"
///           "Phantom Camera" -> "phantom-camera"
fn infer_name_from_asset(title: &str) -> String {
    let base = title.split(" - ").next().unwrap_or(title);
    base.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Derive a dependency name from a git URL.
fn infer_name_from_git(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(url)
        .trim_end_matches(".git")
        .to_lowercase()
}

/// Derive a dependency name from an archive URL's filename, normalising it the
/// same way [`infer_name_from_asset`] does with titles.
///
/// Examples: "https://x.example.com/debug_draw_3d.zip" -> "debug-draw-3d"
///           "https://x.example.com/addon.tar.gz"       -> "addon"
fn infer_name_from_archive_url(url: &str) -> String {
    let path = url
        .split('?')
        .next()
        .unwrap_or(url)
        .split('#')
        .next()
        .unwrap_or(url);
    let filename = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path);
    let stem = filename
        .strip_suffix(".tar.gz")
        .or_else(|| filename.strip_suffix(".tgz"))
        .or_else(|| filename.strip_suffix(".zip"))
        .unwrap_or(filename);
    stem.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Whether `s` looks like an Asset Store reference of the form
/// `publisher_slug/asset_slug[:version]` under the strict slug grammar.
///
/// Only strings the store parser fully understands match, so heuristics like
/// SCP-style git URLs (`user@host:path`) and HTTP URLs (with `://`) never
/// collide with the store route.
fn looks_like_store_spec(s: &str) -> bool {
    let (path, version) = match s.split_once(':') {
        Some((path, version)) => (path, Some(version)),
        None => (s, None),
    };
    let Some((publisher, asset)) = path.split_once('/') else {
        return false;
    };
    if asset.contains('/') {
        return false;
    }
    if validate_asset_store_slug("publisher", publisher).is_err()
        || validate_asset_store_slug("asset", asset).is_err()
    {
        return false;
    }
    match version {
        Some(v) => validate_version_tag(v).is_ok(),
        None => true,
    }
}

/// A parsed Asset Store reference: `publisher/slug` plus a pinned version, if
/// one was specified.
#[derive(Debug)]
struct StoreSpecInput {
    publisher: String,
    asset: String,
    pinned: Option<String>,
}

/// Parse `raw` as an Asset Store reference. Returns `None` when `raw` is not a
/// store reference.
fn parse_store_input(raw: &str) -> Result<Option<StoreSpecInput>> {
    if !looks_like_store_spec(raw) {
        return Ok(None);
    }
    let (path, spec_version) = match raw.split_once(':') {
        Some((path, version)) => (path, Some(version)),
        None => (raw, None),
    };
    let (publisher, asset) = path
        .split_once('/')
        .expect("looks_like_store_spec checked the slash");
    Ok(Some(StoreSpecInput {
        publisher: publisher.to_owned(),
        asset: asset.to_owned(),
        pinned: spec_version.map(str::to_owned),
    }))
}

/// Resolve the release of `publisher/slug` to pin in ggg.toml.
///
/// `pinned` (from the `:version` suffix) selects exactly that release;
/// otherwise the latest stable release compatible with `godot_version` is
/// chosen. Failing either way produces an error listing what was available.
fn resolve_store_release(
    publisher: &str,
    slug: &str,
    pinned: Option<&str>,
    godot_version: &GodotVersion,
) -> Result<StoreRelease> {
    let releases = asset_store::get_releases(publisher, slug, None, false).with_context(|| {
        format!("failed to fetch releases of {publisher}/{slug} from the Godot Asset Store")
    })?;
    let available = if releases.is_empty() {
        "none".to_string()
    } else {
        let mut versions: Vec<&str> = releases.iter().map(|r| r.version.as_str()).collect();
        versions.sort_unstable();
        versions.dedup();
        versions.join(", ")
    };
    match pinned {
        Some(version) => {
            let release = asset_store::select_pinned_release(&releases, version).with_context(|| {
                format!(
                    "no release {version:?} of {publisher}/{slug}; available versions: {available}"
                )
            })?;
            Ok(release.clone())
        }
        None => {
            let release = asset_store::select_latest_compatible(&releases, godot_version)
                .with_context(|| {
                    format!(
                        "no stable release of {publisher}/{slug} is compatible with Godot v{godot_version}; \
                         available versions: {available} \
                         (pin one with publisher/slug:version to use it anyway)"
                    )
                })?;
            Ok(release.clone())
        }
    }
}

/// Confirm, name, and persist a Godot Asset Library dependency, then report it.
fn add_asset_lib_dep(
    config: &mut Config,
    detail: asset_lib::AssetDetail,
    name_arg: Option<&str>,
    yes: bool,
    strip_components: Option<u32>,
    theme: &ColorfulTheme,
) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");

    if name_arg.is_none() && !yes {
        println!("Found: {} (v{})", detail.title, detail.version_string);
        println!("  Author:  {}", detail.author);
        println!("  License: {}", detail.license);
        println!("  Browse:  {}", detail.browse_url);
        println!();
    }
    let name = resolve_name(
        name_arg,
        Some(infer_name_from_asset(&detail.title)),
        yes,
        theme,
    )?;
    if name.is_empty() {
        bail!("dependency name cannot be empty");
    }

    if config.has_dependency(&name) {
        bail!("a dependency named {:?} already exists in ggg.toml", name);
    }

    let dep = Dependency::new(
        &name,
        Source::AssetLib {
            asset_library_id: detail.asset_id,
            strip_components,
        },
        None,
        None,
    );
    config.dependency.push(dep);
    config.save(ggg_toml)?;

    println!(
        "Added {name:?} (asset #{}). Run `ggg sync` to install.",
        detail.asset_id
    );
    Ok(())
}

/// Confirm, name, and persist a Godot Asset Store dependency, then report it.
///
/// The resolved release is always what gets pinned (`publisher/slug:version`).
/// A user who pinned an incompatible release explicitly gets a warning rather
/// than an error - the dep is added anyway.
#[allow(clippy::too_many_arguments)]
fn add_store_asset(
    config: &mut Config,
    publisher: &str,
    slug: &str,
    asset: &StoreAsset,
    release: &StoreRelease,
    godot_version: &GodotVersion,
    name_arg: Option<&str>,
    yes: bool,
    strip_components: Option<u32>,
    theme: &ColorfulTheme,
) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");

    if !release.is_compatible_with(godot_version) {
        let max = release.max_godot_version.as_deref().unwrap_or("latest");
        eprintln!(
            "warning: {publisher}/{slug} v{} requires Godot v{}..{max}, \
             but the project uses Godot v{godot_version}; adding anyway",
            release.version, release.min_godot_version
        );
    }

    if name_arg.is_none() && !yes {
        println!("Found: {} (v{})", asset.name, release.version);
        println!("  Author:  {}", asset.publisher.name);
        println!("  License: {}", asset.license);
        println!(
            "  Godot:   v{}..{}",
            release.min_godot_version,
            release.max_godot_version.as_deref().unwrap_or("latest")
        );
        println!("  Browse:  {}", asset.store_url);
        println!();
    }
    let name = resolve_name(
        name_arg,
        Some(infer_name_from_asset(&asset.name)),
        yes,
        theme,
    )?;
    if name.is_empty() {
        bail!("dependency name cannot be empty");
    }

    if config.has_dependency(&name) {
        bail!("a dependency named {:?} already exists in ggg.toml", name);
    }

    let asset_store_asset: AssetStoreRef = format!("{publisher}/{slug}:{}", release.version)
        .parse()
        .with_context(|| {
            format!(
                "publisher, asset, and version do not form a valid store reference \
                 ({publisher}/{slug}:{})",
                release.version
            )
        })?;
    let dep = Dependency::new(
        &name,
        Source::AssetStore {
            asset_store_asset,
            strip_components,
        },
        None,
        None,
    );
    config.dependency.push(dep);
    config.save(ggg_toml)?;

    println!(
        "Added {name:?} ({publisher}/{slug} v{}). Run `ggg sync` to install.",
        release.version
    );
    Ok(())
}

/// A single addable asset found by the unified search/disambiguation flow.
enum SearchResult {
    /// A Godot Asset Store asset.
    Store(StoreAsset),
    /// A Godot Asset Library search hit.
    Library(AssetSearchResult),
}

/// One search result rendered as a picker item, prefixed with its source so a
/// combined search is unambiguous.
fn hit_display(hit: &SearchResult) -> String {
    match hit {
        SearchResult::Store(asset) => {
            format!(
                "[store] {} ({}/{})",
                asset.name, asset.publisher.slug, asset.slug
            )
        }
        SearchResult::Library(result) => {
            format!(
                "[library] #{} {} - by {}",
                result.asset_id, result.title, result.author
            )
        }
    }
}

/// Turn search results into exactly one chosen asset, shared by the store-only,
/// library-only, and combined paths so they all behave identically:
///
/// - 0 results: error.
/// - 1 result: auto-select.
/// - 2-5 results: interactive picker with a Cancel option.
/// - 6+ results: error suggesting `ggg search` (with `tip` giving the exact
///   add invocation(s) for the current context).
fn disambiguate(
    hits: Vec<SearchResult>,
    total: u32,
    query: &str,
    version_label: &str,
    tip: &str,
    theme: &ColorfulTheme,
) -> Result<SearchResult> {
    match hits.len() {
        0 => bail!("no assets found for {query:?} on Godot {version_label}"),
        _ if total > 5 => {
            bail!("found {total} results for {query:?}; use `ggg search {query}` to browse, {tip}")
        }
        1 => Ok(hits
            .into_iter()
            .next()
            .expect("disambiguate matched on a single hit")),
        _ => {
            // 2-5 results: interactive picker.
            let mut items: Vec<String> = hits.iter().map(hit_display).collect();
            items.push("Cancel".to_owned());

            let choice = Select::with_theme(theme)
                .with_prompt("Select asset")
                .items(&items)
                .default(0)
                .interact()?;

            if choice == hits.len() {
                bail!("cancelled");
            }

            Ok(hits
                .into_iter()
                .nth(choice)
                .expect("choice indexes a rendered item"))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_name_uses_arg_over_default() {
        let theme = ColorfulTheme::default();
        let result =
            resolve_name(Some("custom"), Some("inferred".to_owned()), false, &theme).unwrap();
        assert_eq!(result, "custom");
    }

    #[test]
    fn resolve_name_uses_default_when_yes() {
        let theme = ColorfulTheme::default();
        let result = resolve_name(None, Some("inferred".to_owned()), true, &theme).unwrap();
        assert_eq!(result, "inferred");
    }

    #[test]
    fn resolve_name_arg_takes_precedence_over_yes() {
        let theme = ColorfulTheme::default();
        let result =
            resolve_name(Some("explicit"), Some("inferred".to_owned()), true, &theme).unwrap();
        assert_eq!(result, "explicit");
    }

    #[test]
    fn https_url_with_rev() {
        let (url, rev) = parse_url_rev("https://example.com/gut.git@v9.3.0");
        assert_eq!(url, "https://example.com/gut.git");
        assert_eq!(rev.as_deref(), Some("v9.3.0"));
    }

    #[test]
    fn https_url_without_rev() {
        let (url, rev) = parse_url_rev("https://example.com/gut.git");
        assert_eq!(url, "https://example.com/gut.git");
        assert!(rev.is_none());
    }

    #[test]
    fn ssh_scp_url_with_rev() {
        let (url, rev) = parse_url_rev("git@example.com:user/repo.git@main");
        assert_eq!(url, "git@example.com:user/repo.git");
        assert_eq!(rev.as_deref(), Some("main"));
    }

    #[test]
    fn infer_name_strips_git_suffix_and_lowercases() {
        assert_eq!(infer_name_from_git("https://example.com/Gut.git"), "gut");
    }

    #[test]
    fn infer_name_no_git_suffix() {
        assert_eq!(
            infer_name_from_git("https://example.com/user/my-addon"),
            "my-addon"
        );
    }

    #[test]
    fn infer_name_trailing_slash() {
        assert_eq!(
            infer_name_from_git("https://example.com/user/Repo.git/"),
            "repo"
        );
    }

    #[test]
    fn infer_name_ssh_scp_url() {
        assert_eq!(
            infer_name_from_git("git@example.com:user/phantom-camera.git"),
            "phantom-camera"
        );
    }

    #[test]
    fn infer_name_asset_splits_on_separator() {
        assert_eq!(infer_name_from_asset("GUT - Godot Unit Testing"), "gut");
    }

    #[test]
    fn infer_name_asset_lowercases() {
        assert_eq!(infer_name_from_asset("Phantom Camera"), "phantom-camera");
    }

    #[test]
    fn infer_name_asset_replaces_non_alphanumeric() {
        assert_eq!(
            infer_name_from_asset("My  Awesome__Addon!"),
            "my-awesome-addon"
        );
    }

    #[test]
    fn infer_name_asset_collapses_hyphen_runs() {
        assert_eq!(infer_name_from_asset("a---b"), "a-b");
    }

    #[test]
    fn infer_name_asset_strips_leading_trailing_hyphens() {
        assert_eq!(infer_name_from_asset("- hello -"), "hello");
    }

    #[test]
    fn infer_name_asset_simple() {
        assert_eq!(infer_name_from_asset("Simple Name"), "simple-name");
    }

    #[test]
    fn infer_name_archive_zip() {
        assert_eq!(
            infer_name_from_archive_url("https://example.com/debug_draw_3d.zip"),
            "debug-draw-3d"
        );
    }

    #[test]
    fn infer_name_archive_tar_gz() {
        assert_eq!(
            infer_name_from_archive_url("https://example.com/addon.tar.gz"),
            "addon"
        );
    }

    #[test]
    fn infer_name_archive_tgz_and_query_string() {
        assert_eq!(
            infer_name_from_archive_url("https://example.com/my-addon.tgz?token=123"),
            "my-addon"
        );
    }

    #[test]
    fn infer_name_archive_without_extension_keeps_filename() {
        assert_eq!(
            infer_name_from_archive_url("https://example.com/My-Addon.V1"),
            "my-addon-v1"
        );
    }

    #[test]
    fn infer_name_archive_trailing_slash() {
        assert_eq!(
            infer_name_from_archive_url("https://example.com/dir/addon.zip/"),
            "addon"
        );
    }

    #[test]
    fn store_spec_recognises_bare_reference() {
        assert!(looks_like_store_spec("souleat/godot-xoshiro256-plus-plus"));
    }

    #[test]
    fn store_spec_recognises_versioned_reference() {
        assert!(looks_like_store_spec(
            "souleat/godot-xoshiro256-plus-plus:1.1.0"
        ));
    }

    #[test]
    fn store_spec_rejects_http_and_scp_urls() {
        assert!(!looks_like_store_spec("https://example.com/user/repo.git"));
        assert!(!looks_like_store_spec("git@example.com:user/repo.git"));
        assert!(!looks_like_store_spec("http://example.com/a.zip"));
    }

    #[test]
    fn store_spec_rejects_non_slug_parts() {
        assert!(!looks_like_store_spec("github.com/user/repo"));
        assert!(!looks_like_store_spec("user/repo.git"));
        assert!(!looks_like_store_spec("user/some/asset"));
        assert!(!looks_like_store_spec("UPPER/asset"));
    }

    #[test]
    fn store_spec_rejects_bad_version() {
        assert!(!looks_like_store_spec("user/asset:1 0"));
    }

    #[test]
    fn parse_store_input_accepts_bare_spec() {
        let spec = parse_store_input("souleat/godot-xoshiro256-plus-plus")
            .unwrap()
            .unwrap();
        assert_eq!(spec.publisher, "souleat");
        assert_eq!(spec.asset, "godot-xoshiro256-plus-plus");
        assert!(spec.pinned.is_none());
    }

    #[test]
    fn parse_store_input_accepts_version_suffix() {
        let spec = parse_store_input("user/asset:1.0.0").unwrap().unwrap();
        assert_eq!(spec.pinned.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn parse_store_input_returns_none_for_non_spec() {
        assert!(parse_store_input("guta").unwrap().is_none());
        assert!(
            parse_store_input("https://example.com/x.git")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn hit_display_prefixes_source() {
        let store = SearchResult::Store(StoreAsset {
            slug: "godot-xoshiro256-plus-plus".into(),
            publisher: asset_store::StorePublisher {
                slug: "souleat".into(),
                name: "souleat".into(),
            },
            name: "GodotXoshiro256++".into(),
            license: "MIT".into(),
            store_url: "https://example.com".into(),
        });
        assert_eq!(
            hit_display(&store),
            "[store] GodotXoshiro256++ (souleat/godot-xoshiro256-plus-plus)"
        );

        let library = SearchResult::Library(AssetSearchResult {
            asset_id: 7,
            title: "GUT".into(),
            author: "bitwes".into(),
            license: "MIT".into(),
        });
        assert_eq!(hit_display(&library), "[library] #7 GUT - by bitwes");
    }
}
