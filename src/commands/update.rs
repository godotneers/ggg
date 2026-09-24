//! Implementation of `ggg update`.
//!
//! Checks whether a newer version of a Godot Asset Library or Asset Store
//! dependency is available and, if so, drops the lock entry so that the next
//! `ggg sync` fetches the latest version. For Asset Store deps the newest
//! stable Godot-compatible release is chosen by semantic version comparison
//! (release id only breaks ties between equal version strings) and the pinned
//! version in `ggg.toml` is bumped, since a store dep always pins a version.
//!
//! The command only manages asset library and asset store dependencies, and it
//! requires `ggg.lock` to agree with `ggg.toml`. Before anything is queried or
//! written it runs a drift check ([`LockFile::check_all_dependencies`]) and aborts when the two
//! files disagree - a missing lock entry, a stale lock entry, a dependency
//! whose source kind changed, or lock-key fields edited in `ggg.toml` all
//! mean the locked versions cannot be trusted. `ggg sync` is the
//! reconciliation step. Git and archive dependencies are updated by editing
//! the `rev` or `url` in `ggg.toml`.

use std::cmp::Ordering;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::{Config, Dependency, Source, SourceKind};
use crate::dependency::lockfile::{LockEntry, LockFile};
use crate::godot::asset_lib;
use crate::godot::asset_store;

/// One dependency `ggg update` knows how to check, with the locked version the
/// comparison runs against.
enum UpdateTarget {
    AssetLib {
        asset_library_id: u32,
        locked_version: u32,
    },
    AssetStore {
        publisher: String,
        asset: String,
        locked_version: String,
    },
}

/// Build the update target for a dependency whose lock entry has already been
/// validated by the drift preflight.
fn target_for(dep: &Dependency, entry: &LockEntry) -> UpdateTarget {
    match &dep.source {
        Source::AssetLib {
            asset_library_id, ..
        } => UpdateTarget::AssetLib {
            asset_library_id: *asset_library_id,
            locked_version: entry
                .asset_version
                .expect("preflight rejects asset-lib entries without a recorded version"),
        },
        Source::AssetStore {
            asset_store_asset, ..
        } => UpdateTarget::AssetStore {
            publisher: asset_store_asset.publisher.clone(),
            asset: asset_store_asset.asset.clone(),
            locked_version: asset_store_asset.version.clone(),
        },
        _ => unreachable!("ggg update only builds targets for asset library and store deps"),
    }
}

pub fn run(name: Option<&str>, dry_run: bool) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;
    let lock_path = Path::new("ggg.lock");
    let mut lock = LockFile::load_or_empty(lock_path)?;
    let godot_version = config.project.godot.version.clone();

    // -----------------------------------------------------------------------
    // Preflight: select the targets and reject anything the lock file cannot
    // back up. Nothing is queried or written before this passes.
    // -----------------------------------------------------------------------

    let mut targets: Vec<(&mut Dependency, UpdateTarget)> = Vec::new();

    if let Some(n) = name {
        let dep = config
            .get_dependency_mut(n)
            .with_context(|| format!("no dependency named {:?} in ggg.toml", n))?;
        if !matches!(dep.kind(), SourceKind::AssetLib | SourceKind::AssetStore) {
            bail!(
                "{n:?} is not a Godot Asset Library or Asset Store dependency. \
                 To update git or archive dependencies, edit ggg.toml and run `ggg sync`."
            );
        }
        if let Some(drift) = lock.check_dependency(dep) {
            bail!("{}", drift.message);
        }
        let target = target_for(
            dep,
            lock.get_entry(n)
                .expect("preflight guarantees a lock entry"),
        );
        targets.push((dep, target));
    } else {
        let drifts = lock.check_all_dependencies(&config);
        if !drifts.is_empty() {
            let listing = drifts
                .iter()
                .map(|d| format!("  - {}", d.message))
                .collect::<Vec<_>>()
                .join("\n");
            bail!(
                "ggg.lock and ggg.toml are out of sync:\n{listing}\n\nRun `ggg sync` to reconcile."
            );
        }
        targets.extend(
            config
                .dependency
                .iter_mut()
                .filter(|dep| matches!(dep.kind(), SourceKind::AssetLib | SourceKind::AssetStore))
                .map(|dep| {
                    let target = target_for(
                        dep,
                        lock.get_entry(&dep.name)
                            .expect("preflight guarantees a lock entry"),
                    );
                    (dep, target)
                }),
        );
    }

    if targets.is_empty() {
        println!("No asset library or asset store dependencies in ggg.toml.");
        return Ok(());
    }

    // -----------------------------------------------------------------------
    // Compare pass: query the APIs and report. No writes.
    // -----------------------------------------------------------------------

    let mut any_updated = false;

    for (dep, target) in &mut targets {
        let dep_name = dep.name.clone();

        match target {
            UpdateTarget::AssetLib {
                asset_library_id,
                locked_version,
            } => {
                let locked_version = *locked_version;

                let detail = asset_lib::get_asset(*asset_library_id).with_context(|| {
                    format!(
                        "failed to fetch asset {:?} (id={asset_library_id}) from the Godot Asset Library",
                        dep_name,
                    )
                })?;

                if detail.version <= locked_version {
                    println!("{}: up to date (v{}).", dep_name, detail.version_string);
                    continue;
                }

                if dry_run {
                    println!(
                        "{}: update available: version {} -> v{}.",
                        dep_name, locked_version, detail.version_string,
                    );
                } else {
                    lock.remove(&dep_name);
                    println!(
                        "{}: version {} -> v{} - run `ggg sync` to install.",
                        dep_name, locked_version, detail.version_string,
                    );
                }
                any_updated = true;
            }

            UpdateTarget::AssetStore {
                publisher,
                asset,
                locked_version: pinned,
            } => {
                let releases =
                    asset_store::get_releases(publisher, asset, None, false).with_context(|| {
                        format!(
                            "failed to fetch releases of {publisher}/{asset} from the Godot Asset Store"
                        )
                    })?;
                let Some(latest) = asset_store::select_latest_compatible(&releases, &godot_version)
                else {
                    println!(
                        "{}: no stable release of {publisher}/{asset} is compatible with Godot v{godot_version}.",
                        dep_name
                    );
                    continue;
                };

                if asset_store::cmp_store_versions(&latest.version, pinned) != Ordering::Greater {
                    println!("{}: up to date (v{}).", dep_name, pinned);
                    continue;
                }

                if dry_run {
                    println!(
                        "{}: update available: version {} -> v{} (release id {}).",
                        dep_name, pinned, latest.version, latest.id,
                    );
                } else {
                    let Source::AssetStore {
                        asset_store_asset, ..
                    } = &mut dep.source
                    else {
                        unreachable!("update target was built from the same AssetStore source")
                    };
                    asset_store_asset.version = latest.version.clone();
                    lock.remove(&dep_name);
                    println!(
                        "{}: version {} -> v{} - run `ggg sync` to install.",
                        dep_name, pinned, latest.version,
                    );
                }
                any_updated = true;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Apply: persist the bumped pins and pruned lock entries.
    // -----------------------------------------------------------------------

    if !dry_run && any_updated {
        lock.save(lock_path).context("failed to write ggg.lock")?;
        config.save(ggg_toml).context("failed to write ggg.toml")?;
    }

    Ok(())
}
