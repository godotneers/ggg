//! Implementation of `ggg search`.
//!
//! Searches the Godot Asset Library and prints a result table.  The Godot
//! version from `ggg.toml` is used to filter results unless overridden with
//! `--godot-version`.

use std::path::Path;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::godot::asset_lib;

pub fn run(query: &str, godot_version_override: Option<&str>) -> Result<()> {
    let godot_version = match godot_version_override {
        Some(v) => v.to_owned(),
        None => {
            let toml = Path::new("ggg.toml");
            if toml.exists() {
                let config = Config::load(toml)?;
                let v = &config.project.godot.version;
                format!("{}.{}", v.major, v.minor)
            } else {
                // No ggg.toml - search without a version filter by using a
                // broad version string.  The user can override with
                // --godot-version if they need a specific version.
                String::new()
            }
        }
    };

    let (results, total) = asset_lib::search(query, &godot_version)
        .context("failed to search the Godot Asset Library")?;

    let version_label = if godot_version.is_empty() {
        String::new()
    } else {
        format!(" on Godot {godot_version}")
    };

    if results.is_empty() {
        println!("No results for {:?}{version_label}.", query);
        return Ok(());
    }

    // Column widths.
    let id_w     = results.iter().map(|r| digits(r.asset_id)).max().unwrap_or(2).max(2);
    let title_w  = results.iter().map(|r| r.title.len()).max().unwrap_or(5).max(5).min(40);
    let author_w = results.iter().map(|r| r.author.len()).max().unwrap_or(6).max(6).min(20);

    println!(
        "{:id_w$}  {:<title_w$}  {:<author_w$}  License",
        "ID", "Title", "Author",
    );
    println!("{}", "-".repeat(id_w + 2 + title_w + 2 + author_w + 2 + 7));

    for r in &results {
        println!(
            "{:id_w$}  {:<title_w$}  {:<author_w$}  {}",
            r.asset_id,
            truncate(&r.title, title_w),
            truncate(&r.author, author_w),
            r.license,
        );
    }

    let shown = results.len();
    if total as usize > shown {
        println!();
        println!(
            "Showing {shown} of {total} results{version_label}. \
             Use `ggg search` with a more specific query to narrow results."
        );
    } else {
        println!();
        println!("Found {total} result{}{version_label}.", if total == 1 { "" } else { "s" });
    }

    println!("Use `ggg add asset --id <N>` to add a specific asset.");

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        format!("{}..", &s[..max.saturating_sub(2)])
    }
}

fn digits(n: u32) -> usize {
    if n == 0 { 1 } else { n.ilog10() as usize + 1 }
}
