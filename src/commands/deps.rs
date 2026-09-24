//! Implementation of `ggg deps`.
//!
//! Prints the dependencies declared in `ggg.toml` as a table showing each
//! dep's name, source type, and version information.

use std::path::Path;

use anyhow::Result;

use crate::config::{Config, Source};
use crate::utils::output::print_table;

pub fn run() -> Result<()> {
    let config = Config::load(Path::new("ggg.toml"))?;

    if config.dependency.is_empty() {
        println!("No dependencies in ggg.toml.");
        return Ok(());
    }

    print_table(
        &["Name", "Type", "Version / Source"],
        &config.dependency,
        &[
            Box::new(|dep| dep.name.to_string()),
            Box::new(|dep| match &dep.source {
                Source::Git { .. } => "git".to_string(),
                Source::Archive { .. } => "archive".to_string(),
                Source::AssetLib { .. } => "asset-lib".to_string(),
                Source::AssetStore { .. } => "asset-store".to_string(),
            }),
            Box::new(|dep| match &dep.source {
                Source::Git { rev, .. } => rev.to_string(),
                Source::Archive { url, .. } => url.to_string(),
                Source::AssetLib {
                    asset_library_id, ..
                } => format!("asset #{asset_library_id}"),
                Source::AssetStore {
                    asset_store_asset, ..
                } => asset_store_asset.to_string(),
            }),
        ],
    );
    Ok(())
}
