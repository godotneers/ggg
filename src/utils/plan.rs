//! Shared sync planning logic used by both `ggg sync` and `ggg diff`.
//!
//! [`resolve_and_plan`] resolves and downloads every dependency declared in
//! `ggg.toml` and computes [`InstallPlan`]s and a [`CleanupPlan`] without
//! writing anything to the project directory.

use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{Config, DepKind};
use crate::dependency::cache::DependencyCache;
use crate::dependency::download::download_dependency;
use crate::dependency::install::{plan_cleanup, plan_install, CleanupPlan, InstallPlan};
use crate::dependency::lockfile::LockFile;
use crate::dependency::resolver::resolve;
use crate::dependency::state::{LocalState, StateEntry};
use crate::dependency::ResolvedDependency;

/// One dependency's resolved identity together with its install plan.
pub struct DepWork {
    pub resolved: ResolvedDependency,
    /// Short human-readable note about how the SHA was obtained, e.g.
    /// "locked 5405e88f472c" or "resolved 9f3edeb19df6".
    pub resolve_note: String,
    pub plan: InstallPlan,
}

/// The full plan for a sync run: one [`DepWork`] per dependency plus the
/// stale-file cleanup plan.
pub struct SyncPlan {
    pub works: Vec<DepWork>,
    pub cleanup: CleanupPlan,
}

/// Resolve and cache a single dependency, returning its resolved form and a
/// short human-readable note (e.g. `"locked a1b2c3d4e5f6"` or
/// `"resolved a1b2c3d4e5f6"`).
///
/// Downloads and installs into the cache on a miss.  Does **not** write the
/// lock file; the caller is responsible for that.
pub fn resolve_dependency(
    dep: &crate::config::Dependency,
    lock: &LockFile,
    dep_cache: &DependencyCache,
) -> Result<(ResolvedDependency, String)> {
    match dep.kind() {
        DepKind::Git { git, rev } => resolve_git_dependency(dep, git, rev, lock, dep_cache),
        DepKind::Archive { url, .. } => resolve_archive_dependency(dep, url, lock, dep_cache),
        DepKind::AssetLib { asset_id } => resolve_asset_lib_dependency(dep, asset_id, lock, dep_cache),
    }
}

/// Resolve, download (if needed), and plan the install for every dependency
/// in `config`.  Nothing is written to `project_root`.
///
/// Pass `force = true` to suppress conflict detection (mirrors `--force` in
/// `ggg sync`).
pub fn resolve_and_plan(
    config: &Config,
    lock: &LockFile,
    old_state: &LocalState,
    state_present: bool,
    dep_cache: &DependencyCache,
    project_root: &Path,
    force: bool,
) -> Result<SyncPlan> {
    let mut works: Vec<DepWork> = Vec::new();

    for dep in &config.dependency {
        let (resolved, resolve_note) = resolve_dependency(dep, lock, dep_cache)?;

        let cache_dir = dep_cache.entry_path(&resolved);

        let force_overwrite = config.sync.as_ref()
            .map(|s| s.force_overwrite.as_slice())
            .unwrap_or(&[]);

        let plan = plan_install(&resolved, &cache_dir, project_root, old_state, force, force_overwrite)
            .with_context(|| format!("failed to plan install for {:?}", dep.name))?;

        works.push(DepWork { resolved, resolve_note, plan });
    }

    let new_entries: Vec<StateEntry> =
        works.iter().map(|w| w.plan.entry.clone()).collect();

    let cleanup =
        plan_cleanup(old_state, &new_entries, project_root, state_present, force)?;

    Ok(SyncPlan { works, cleanup })
}

// ---------------------------------------------------------------------------
// Per-dep-type helpers
// ---------------------------------------------------------------------------

fn resolve_git_dependency(
    dep: &crate::config::Dependency,
    git: &str,
    rev: &str,
    lock: &LockFile,
    dep_cache: &DependencyCache,
) -> Result<(ResolvedDependency, String)> {
    let used_lock = lock.locked_sha(&dep.name, git, rev).is_some();
    let (mut resolved, mut resolve_note) =
        if let Some(sha) = lock.locked_sha(&dep.name, git, rev) {
            let note = format!("locked {}", &sha[..12]);
            (ResolvedDependency { dep: dep.clone(), sha: sha.to_owned(), resolved_url: None, asset_version: None }, note)
        } else {
            let r = resolve(dep)
                .with_context(|| format!("failed to resolve dependency {:?}", dep.name))?;
            let note = format!("resolved {}", &r.sha[..12]);
            (r, note)
        };

    if !dep_cache.contains(&resolved) {
        println!("  {} - downloading...", dep.name);
        let download_result = download_dependency(&resolved);

        let repo_path = match download_result {
            Err(e) if used_lock => {
                eprintln!(
                    "  warning: locked commit {} for {:?} is no longer available ({}); \
                     re-resolving from {:?}",
                    &resolved.sha[..12], dep.name, e, rev
                );
                resolved = resolve(dep).with_context(|| {
                    format!("failed to re-resolve dependency {:?}", dep.name)
                })?;
                resolve_note = format!("re-resolved {}", &resolved.sha[..12]);
                download_dependency(&resolved).with_context(|| {
                    format!("failed to download dependency {:?}", dep.name)
                })?
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("failed to download dependency {:?}", dep.name)
                });
            }
            Ok(path) => path,
        };

        dep_cache
            .install(&resolved, &repo_path)
            .with_context(|| format!("failed to cache dependency {:?}", dep.name))?;

        let _ = std::fs::remove_dir_all(&repo_path);
    }

    Ok((resolved, resolve_note))
}

fn resolve_asset_lib_dependency(
    dep: &crate::config::Dependency,
    asset_id: u32,
    lock: &LockFile,
    dep_cache: &DependencyCache,
) -> Result<(ResolvedDependency, String)> {
    if let Some(entry) = lock.locked_asset_lib(&dep.name, asset_id) {
        let url = entry.url.as_deref()
            .with_context(|| format!("lock entry for {:?} is missing a url field", dep.name))?;
        let archive_sha = entry.archive_sha.as_deref()
            .with_context(|| format!("lock entry for {:?} is missing an archive_sha field", dep.name))?;

        let version_label = entry.asset_version
            .map(|v| format!(" (version {})", v))
            .unwrap_or_default();
        let note = format!("locked{}", version_label);

        let resolved = ResolvedDependency {
            dep: dep.clone(),
            sha: archive_sha.to_owned(),
            resolved_url: Some(url.to_owned()),
            asset_version: entry.asset_version,
        };

        if !dep_cache.contains(&resolved) {
            // Cache was cleared - re-download from the locked URL and verify.
            println!("  {} - re-downloading (cache miss)...", dep.name);
            let downloaded_sha = dep_cache
                .install_asset_lib(&dep.name, url, Some(archive_sha))
                .with_context(|| format!("failed to re-download {:?}", dep.name))?;
            if downloaded_sha != archive_sha {
                anyhow::bail!(
                    "archive for {:?} has changed since the lock file was written\n\
                     locked:     {archive_sha}\n\
                     downloaded: {downloaded_sha}\n\
                     If this is intentional, remove the lock entry and re-run ggg sync.",
                    dep.name,
                );
            }
        }

        return Ok((resolved, note));
    }

    // No lock entry - fetch current version from the asset library API.
    println!("  {} - fetching from Godot Asset Library...", dep.name);
    let detail = crate::godot::asset_lib::get_asset(asset_id)
        .with_context(|| format!(
            "failed to fetch asset {:?} (id={}) from the Godot Asset Library",
            dep.name, asset_id,
        ))?;

    let archive_sha = dep_cache
        .install_asset_lib(&dep.name, &detail.download_url, detail.download_hash.as_deref())
        .with_context(|| format!("failed to download {:?}", dep.name))?;

    let note = format!("downloaded v{}", detail.version_string);

    let resolved = ResolvedDependency {
        dep: dep.clone(),
        sha: archive_sha,
        resolved_url: Some(detail.download_url),
        asset_version: Some(detail.version),
    };

    Ok((resolved, note))
}

fn resolve_archive_dependency(
    dep: &crate::config::Dependency,
    url: &str,
    lock: &LockFile,
    dep_cache: &DependencyCache,
) -> Result<(ResolvedDependency, String)> {
    if let Some(locked_sha) = lock.locked_archive_sha(&dep.name, url) {
        let resolved = ResolvedDependency { dep: dep.clone(), sha: locked_sha.to_owned(), resolved_url: None, asset_version: None };
        let note = format!("locked {}", &locked_sha[..8]);

        if !dep_cache.contains(&resolved) {
            // Cache was cleared - re-download and verify the content hasn't changed.
            println!("  {} - re-downloading (cache miss)...", dep.name);
            let downloaded_sha = dep_cache
                .install_archive(dep)
                .with_context(|| format!("failed to download {:?}", dep.name))?;
            if downloaded_sha != locked_sha {
                anyhow::bail!(
                    "archive for {:?} has changed since the lock file was written\n\
                     locked:     {locked_sha}\n\
                     downloaded: {downloaded_sha}\n\
                     If this is intentional, remove the lock entry and re-run ggg sync.",
                    dep.name,
                );
            }
        }

        Ok((resolved, note))
    } else {
        // No lock entry - fresh download.
        println!("  {} - downloading...", dep.name);
        let archive_sha = dep_cache
            .install_archive(dep)
            .with_context(|| format!("failed to download {:?}", dep.name))?;
        let resolved = ResolvedDependency { dep: dep.clone(), sha: archive_sha.clone(), resolved_url: None, asset_version: None };
        let note = format!("downloaded {}", &archive_sha[..8]);
        Ok((resolved, note))
    }
}
