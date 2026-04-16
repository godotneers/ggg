//! Implementation of `ggg update`.
//!
//! Checks whether a newer version of a Godot Asset Library dependency is
//! available and, if so, drops the lock entry so that the next `ggg sync`
//! fetches the latest version.
//!
//! Only `asset_id`-sourced dependencies support this command.  Git and archive
//! dependencies are updated by editing the `rev` or `url` in `ggg.toml`.

use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::config::{Config, DepKind};
use crate::dependency::lockfile::LockFile;
use crate::godot::asset_lib;

pub fn run(name: Option<&str>, dry_run: bool) -> Result<()> {
    let config = Config::load(Path::new("ggg.toml"))?;
    let lock_path = Path::new("ggg.lock");
    let mut lock = LockFile::load_or_empty(lock_path)?;

    let deps_to_check: Vec<_> = if let Some(n) = name {
        let dep = config
            .get_dependency(n)
            .with_context(|| format!("no dependency named {:?} in ggg.toml", n))?;
        if !matches!(dep.kind(), DepKind::AssetLib { .. }) {
            bail!(
                "{n:?} is not a Godot Asset Library dependency. \
                 To update git or archive dependencies, edit ggg.toml and run `ggg sync`."
            );
        }
        vec![dep]
    } else {
        config.dependency.iter()
            .filter(|d| matches!(d.kind(), DepKind::AssetLib { .. }))
            .collect()
    };

    if deps_to_check.is_empty() {
        println!("No asset library dependencies in ggg.toml.");
        return Ok(());
    }

    let mut any_updated = false;

    for dep in &deps_to_check {
        let DepKind::AssetLib { asset_id } = dep.kind() else { unreachable!() };

        let locked_version = lock
            .locked_asset_lib(&dep.name, asset_id)
            .and_then(|e| e.asset_version);

        if locked_version.is_none() {
            println!(
                "{}: no lock entry - run `ggg sync` to install and lock the current version.",
                dep.name
            );
            continue;
        }
        let locked_version = locked_version.unwrap();

        let detail = asset_lib::get_asset(asset_id)
            .with_context(|| format!(
                "failed to fetch asset {:?} (id={asset_id}) from the Godot Asset Library",
                dep.name,
            ))?;

        if detail.version <= locked_version {
            println!("{}: up to date (v{}).", dep.name, detail.version_string);
            continue;
        }

        if dry_run {
            println!(
                "{}: update available: version {} -> v{}.",
                dep.name, locked_version, detail.version_string,
            );
        } else {
            lock.remove(&dep.name);
            println!(
                "{}: version {} -> v{} - run `ggg sync` to install.",
                dep.name, locked_version, detail.version_string,
            );
        }
        any_updated = true;
    }

    if !dry_run && any_updated {
        lock.save(lock_path).context("failed to write ggg.lock")?;
    }

    Ok(())
}
