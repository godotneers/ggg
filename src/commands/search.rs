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
    let id_w = results
        .iter()
        .map(|r| digits(r.asset_id))
        .max()
        .unwrap_or(2)
        .max(2);
    let title_w = results
        .iter()
        .map(|r| r.title.len())
        .max()
        .unwrap_or(5)
        .clamp(5, 40);
    let author_w = results
        .iter()
        .map(|r| r.author.len())
        .max()
        .unwrap_or(6)
        .clamp(6, 20);

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
        println!(
            "Found {total} result{}{version_label}.",
            if total == 1 { "" } else { "s" }
        );
    }

    println!("Use `ggg add asset --id <N>` to add a specific asset.");

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        // Cut at a UTF-8 character boundary within the byte budget so
        // multi-byte characters are never sliced in half.
        let budget = max.saturating_sub(2);
        let cut = s
            .char_indices()
            .take_while(|&(i, _)| i <= budget)
            .map(|(i, _)| i)
            .last()
            .unwrap_or(0);
        format!("{}..", &s[..cut])
    }
}

fn digits(n: u32) -> usize {
    if n == 0 { 1 } else { n.ilog10() as usize + 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_counts_decimal_places() {
        assert_eq!(digits(0), 1);
        assert_eq!(digits(1), 1);
        assert_eq!(digits(9), 1);
        assert_eq!(digits(10), 2);
        assert_eq!(digits(99), 2);
        assert_eq!(digits(100), 3);
        assert_eq!(digits(999), 3);
        assert_eq!(digits(1_000), 4);
        assert_eq!(digits(u32::MAX), 10);
    }

    #[test]
    fn truncate_leaves_short_strings_untouched() {
        assert_eq!(truncate("hi", 10), "hi");
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_appends_dots_at_byte_cut() {
        assert_eq!(truncate("hello world", 5), "hel..");
        assert_eq!(truncate("hello world", 2), "..");
        assert_eq!(truncate("hello world", 1), "..");
    }

    #[test]
    fn truncate_survives_multi_byte_characters() {
        // "你" is 3 bytes, so a byte-only cut would panic by slicing through
        // the middle of a character. The cut must land on a char boundary.
        assert_eq!(truncate("你好世界", 5), "你..");
        assert_eq!(truncate("abc你def", 5), "abc..");
    }
}
