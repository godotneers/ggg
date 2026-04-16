//! Implementation of `ggg deps`.
//!
//! Prints the dependencies declared in `ggg.toml` as a table showing each
//! dep's name, source type, and version information.

use std::path::Path;

use anyhow::Result;

use crate::config::{Config, DepKind};

pub fn run() -> Result<()> {
    let config = Config::load(Path::new("ggg.toml"))?;

    if config.dependency.is_empty() {
        println!("No dependencies in ggg.toml.");
        return Ok(());
    }

    // Column widths.
    let name_w = config.dependency.iter()
        .map(|d| d.name.len())
        .max().unwrap_or(4)
        .max(4);
    let type_w = 7; // "archive" is the longest type label

    println!("{:<name_w$}  {:<type_w$}  Version / Source", "Name", "Type");
    println!("{}", "-".repeat(name_w + 2 + type_w + 2 + 16));

    for dep in &config.dependency {
        let (type_label, version_info) = match dep.kind() {
            DepKind::Git { git, rev } => {
                let short_url = git.trim_end_matches(".git")
                    .rsplit('/')
                    .next()
                    .unwrap_or(git);
                ("git", format!("{rev}  ({short_url})"))
            }
            DepKind::Archive { url, .. } => {
                // Show just the filename part of the URL.
                let filename = url.rsplit('/').next().unwrap_or(url);
                ("archive", filename.to_owned())
            }
            DepKind::AssetLib { asset_id } => {
                ("asset", format!("asset #{asset_id}"))
            }
        };
        println!("{:<name_w$}  {:<type_w$}  {}", dep.name, type_label, version_info);
    }

    Ok(())
}
