//! Shared sync planning logic used by both `ggg sync` and `ggg diff`.
//!
//! [`build_plan`] resolves and downloads every dependency declared in
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

/// Resolve, download (if needed), and plan the install for every dependency
/// in `config`.  Nothing is written to `project_root`.
///
/// Pass `force = true` to suppress conflict detection (mirrors `--force` in
/// `ggg sync`).
pub fn build_plan(
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
        let (resolved, resolve_note) = match dep.kind() {
            DepKind::Git { git, rev } => {
                plan_git_dep(dep, git, rev, lock, dep_cache)?
            }
            DepKind::Archive { url, .. } => {
                plan_archive_dep(dep, url, lock, dep_cache)?
            }
        };

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

fn plan_git_dep(
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
            (ResolvedDependency { dep: dep.clone(), sha: sha.to_owned() }, note)
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

fn plan_archive_dep(
    dep: &crate::config::Dependency,
    url: &str,
    lock: &LockFile,
    dep_cache: &DependencyCache,
) -> Result<(ResolvedDependency, String)> {
    if let Some(locked_sha) = lock.locked_archive_sha(&dep.name, url) {
        let resolved = ResolvedDependency { dep: dep.clone(), sha: locked_sha.to_owned() };
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
        let resolved = ResolvedDependency { dep: dep.clone(), sha: archive_sha.clone() };
        let note = format!("downloaded {}", &archive_sha[..8]);
        Ok((resolved, note))
    }
}
