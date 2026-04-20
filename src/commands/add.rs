//! Implementation of `ggg add`.
//!
//! Three subcommands:
//!
//! - `ggg add git <url>[@rev]` - adds a git dependency, resolves the rev
//!   against the remote before writing.
//! - `ggg add archive <url>` - adds an archive dependency; no network call at
//!   add time (the sha256 field, if provided, is verified on first sync).
//! - `ggg add asset <query|id>` - searches the Godot Asset Library and adds
//!   the matching dep by asset ID.
//!
//! The bare `ggg add <input>` form is kept as a convenience: archive URLs
//! route to the archive path, git URLs route to git, and anything else is
//! treated as an asset library query or ID.

use std::path::Path;

use anyhow::{bail, Context, Result};
use dialoguer::{Input, Select, theme::ColorfulTheme};
use indicatif::ProgressBar;

use crate::config::{Config, Dependency};
use crate::dependency::resolver;
use crate::godot::asset_lib;

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

    let dep = Dependency::new_git(&name, &git, &rev);
    let spinner = ProgressBar::new_spinner();
    spinner.set_message(format!("Resolving {rev}..."));
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    let resolved = resolver::resolve(&dep)
        .with_context(|| format!("failed to resolve {:?} from {git:?}", rev))?;
    spinner.finish_and_clear();

    config.dependency.push(dep);
    config.save(ggg_toml)?;

    println!("Added {name:?} ({rev}) resolved to {}", &resolved.sha[..12]);
    println!("Run `ggg sync` to install it.");
    Ok(())
}

pub fn run_archive(archive_url: Option<&str>, name_arg: Option<&str>, strip_components: Option<u32>, sha256: Option<&str>) -> Result<()> {
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

    let name = resolve_name(name_arg, None, false, &theme)?;
    if name.is_empty() {
        bail!("dependency name cannot be empty");
    }

    if config.dependency.iter().any(|d| d.name == name) {
        bail!("a dependency named {:?} already exists in ggg.toml", name);
    }

    let mut dep = Dependency::new_archive(&name, &url);
    dep.sha256 = sha256.map(str::to_owned);
    dep.strip_components = strip_components;

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
/// - URL-like strings (`://`, `.git`, SCP-style) -> git dep.
/// - Anything else (plain name or numeric ID) -> asset library search.
pub fn run_bare(input: &str, yes: bool) -> Result<()> {
    if input.ends_with(".zip") || input.ends_with(".tar.gz") || input.ends_with(".tgz") {
        run_archive(Some(input), None, None, None)
    } else if input.contains("://") || input.ends_with(".git") || input.contains(':') {
        run_git(Some(input), None, yes)
    } else {
        run_asset(Some(input), None, None, yes)
    }
}

/// Handle `ggg add asset [<query-or-id>] [--id <N>]`.
///
/// If `id_override` is given the asset is fetched directly.  Otherwise
/// `query` is used as a search term; a pure number is treated as an ID.
///
/// - 0 results: error.
/// - 1 result: confirmation prompt (skipped with `--yes`).
/// - 2-5 results: interactive picker with a Cancel option.
/// - 6+ results: error suggesting `ggg search`.
pub fn run_asset(query: Option<&str>, id_override: Option<u32>, name_arg: Option<&str>, yes: bool) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;
    let theme = ColorfulTheme::default();

    let godot_version = {
        let v = &config.project.godot.version;
        format!("{}.{}", v.major, v.minor)
    };

    // Resolve to a single AssetDetail.
    let detail = if let Some(id) = id_override {
        asset_lib::get_asset(id)
            .with_context(|| format!("failed to fetch asset id {id} from the Godot Asset Library"))?
    } else {
        let q = match query {
            Some(q) => q.to_owned(),
            None => Input::with_theme(&theme)
                .with_prompt("Asset name or ID")
                .interact_text()?,
        };

        // A pure number is treated as a direct asset ID.
        if let Ok(id) = q.trim().parse::<u32>() {
            asset_lib::get_asset(id)
                .with_context(|| format!("failed to fetch asset id {id} from the Godot Asset Library"))?
        } else {
            let (results, total) = asset_lib::search(&q, &godot_version)
                .context("failed to search the Godot Asset Library")?;

            match results.len() {
                0 => bail!(
                    "no assets found for {:?} on Godot {godot_version}",
                    q
                ),
                _ if total > 5 => bail!(
                    "found {total} results for {:?}; use `ggg search {q}` to browse, \
                     then `ggg add asset --id <N>` to add a specific one",
                    q
                ),
                1 => {
                    asset_lib::get_asset(results[0].asset_id)
                        .with_context(|| "failed to fetch asset details from the Godot Asset Library".to_string())?
                }
                _ => {
                    // 2-5 results: interactive picker.
                    let mut items: Vec<String> = results
                        .iter()
                        .map(|r| format!("#{} {} - by {} [{}]", r.asset_id, r.title, r.author, r.license))
                        .collect();
                    items.push("Cancel".to_owned());

                    let choice = Select::with_theme(&theme)
                        .with_prompt("Select asset")
                        .items(&items)
                        .default(0)
                        .interact()?;

                    if choice == results.len() {
                        bail!("cancelled");
                    }

                    asset_lib::get_asset(results[choice].asset_id)
                        .with_context(|| "failed to fetch asset details from the Godot Asset Library".to_string())?
                }
            }
        }
    };

    // Confirm and add.
    let default_name = infer_name_from_asset(&detail.title);

    if name_arg.is_none() && !yes {
        println!("Found: {} (v{})", detail.title, detail.version_string);
        println!("  Author:  {}", detail.author);
        println!("  License: {}", detail.license);
        println!("  Browse:  {}", detail.browse_url);
        println!();
    }
    let name = resolve_name(name_arg, Some(default_name), yes, &theme)?;

    if name.is_empty() {
        bail!("dependency name cannot be empty");
    }

    if config.has_dependency(&name) {
        bail!("a dependency named {:?} already exists in ggg.toml", name);
    }

    let dep = Dependency::new_asset_lib(&name, detail.asset_id);
    config.dependency.push(dep);
    config.save(ggg_toml)?;

    println!("Added {name:?} (asset #{}). Run `ggg sync` to install.", detail.asset_id);
    Ok(())
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
        let looks_like_url = left.contains("://")
            || left.ends_with(".git")
            || left.contains(':');
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_name_uses_arg_over_default() {
        let theme = ColorfulTheme::default();
        let result = resolve_name(Some("custom"), Some("inferred".to_owned()), false, &theme).unwrap();
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
        let result = resolve_name(Some("explicit"), Some("inferred".to_owned()), true, &theme).unwrap();
        assert_eq!(result, "explicit");
    }

    #[test]
    fn https_url_with_rev() {
        let (url, rev) = parse_url_rev("https://github.com/bitwes/Gut.git@v9.3.0");
        assert_eq!(url, "https://github.com/bitwes/Gut.git");
        assert_eq!(rev.as_deref(), Some("v9.3.0"));
    }

    #[test]
    fn https_url_without_rev() {
        let (url, rev) = parse_url_rev("https://github.com/bitwes/Gut.git");
        assert_eq!(url, "https://github.com/bitwes/Gut.git");
        assert!(rev.is_none());
    }

    #[test]
    fn ssh_scp_url_with_rev() {
        let (url, rev) = parse_url_rev("git@github.com:user/repo.git@main");
        assert_eq!(url, "git@github.com:user/repo.git");
        assert_eq!(rev.as_deref(), Some("main"));
    }

    #[test]
    fn infer_name_strips_git_suffix_and_lowercases() {
        assert_eq!(infer_name_from_git("https://github.com/bitwes/Gut.git"), "gut");
    }

    #[test]
    fn infer_name_no_git_suffix() {
        assert_eq!(infer_name_from_git("https://github.com/user/my-addon"), "my-addon");
    }

    #[test]
    fn infer_name_trailing_slash() {
        assert_eq!(infer_name_from_git("https://github.com/user/Repo.git/"), "repo");
    }

    #[test]
    fn infer_name_ssh_scp_url() {
        assert_eq!(infer_name_from_git("git@github.com:user/phantom-camera.git"), "phantom-camera");
    }
}
