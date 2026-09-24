//! Implementation of `ggg search`.
//!
//! Searches the Godot Asset Store (default) or the Godot Asset Library and
//! prints a result table.  The Godot version from `ggg.toml` is used to filter
//! results unless overridden with `--godot-version`.

use std::path::Path;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::godot::asset_lib;
use crate::godot::asset_store;
use crate::godot::release::GodotVersion;
use crate::utils::output::print_table;

/// The source `ggg search` queries, selected with `--source`.
#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum SearchSource {
    /// The Godot Asset Store (default); the store is the future home for
    /// Godot addons and the asset library is slated for removal.
    #[default]
    AssetStore,
    /// The Godot Asset Library.
    AssetLibrary,
}

pub fn run(query: &str, godot_version_override: Option<&str>, source: SearchSource) -> Result<()> {
    let godot_version: Option<GodotVersion> = match godot_version_override {
        Some(v) => Some(
            v.parse()
                .with_context(|| format!("invalid --godot-version override {v:?}"))?,
        ),
        None => {
            let toml = Path::new("ggg.toml");
            if toml.exists() {
                let config = Config::load(toml)?;
                Some(config.project.godot.version.clone())
            } else {
                // No ggg.toml - search without a version filter by passing
                // `None`.  The user can override with --godot-version if they
                // need a specific version.
                None
            }
        }
    };

    let version_label = godot_version
        .as_ref()
        .map(|v| format!(" on Godot {}", v.major_minor()))
        .unwrap_or_default();

    let outcome = match source {
        SearchSource::AssetStore => run_store(query, godot_version.as_ref())?,
        SearchSource::AssetLibrary => run_library(query, godot_version.as_ref())?,
    };

    if outcome.shown == 0 {
        println!("No results for {:?}{version_label}.", query);
        return Ok(());
    }

    print_result_footer(outcome.total, outcome.shown, &version_label);
    println!("{}", outcome.hint);

    Ok(())
}

/// What one source search printed, so `run` can append the shared footer.
struct SearchOutcome {
    /// Total number of matches reported by the source.
    total: u32,
    /// Number of results displayed on this page (the first page only).
    shown: usize,
    /// The "Use `ggg add ...`" hint specific to the source.
    hint: &'static str,
}

/// Render results from the Godot Asset Store.
///
/// The first column shows the `publisher_slug/asset_slug` reference the user
/// would type into `ggg add asset-store`, not the publisher's display name.
fn run_store(query: &str, godot_version: Option<&GodotVersion>) -> Result<SearchOutcome> {
    let (results, total) = asset_store::search(query, godot_version)
        .context("failed to search the Godot Asset Store")?;

    print_table(
        &["publisher/slug", "Name", "License"],
        &results,
        &[
            Box::new(|result| format!("{}/{}", result.publisher.slug, result.slug)),
            Box::new(|result| result.name.clone()),
            Box::new(|result| result.license.clone()),
        ],
    );
    Ok(SearchOutcome {
        total,
        shown: results.len(),
        hint: "Use `ggg add asset-store <publisher>/<slug>` to add a specific asset.",
    })
}

/// Render results from the Godot Asset Library.
fn run_library(query: &str, godot_version: Option<&GodotVersion>) -> Result<SearchOutcome> {
    let (results, total) = asset_lib::search(query, godot_version)
        .context("failed to search the Godot Asset Library")?;

    print_table(
        &["ID", "Title", "Author", "License"],
        &results,
        &[
            Box::new(|result| result.asset_id.to_string()),
            Box::new(|result| result.title.to_string()),
            Box::new(|result| result.author.to_string()),
            Box::new(|result| result.license.to_string()),
        ],
    );
    Ok(SearchOutcome {
        total,
        shown: results.len(),
        hint: "Use `ggg add asset-library --id <N>` to add a specific asset.",
    })
}

/// The shared "Showing X of Y" / "Found N results" footer.
fn print_result_footer(total: u32, shown: usize, version_label: &str) {
    if total as usize > shown {
        println!();
        println!(
            "Showing {shown} of {total} results{version_label}. \
             Use `ggg search` with a more specific query to narrow results."
        );
    } else {
        println!();
        println!(
            "Found {total} result{}{version_label}.",
            if total == 1 { "" } else { "s" }
        );
    }
}
