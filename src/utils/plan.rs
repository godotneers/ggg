//! Shared sync planning logic used by both `ggg sync` and `ggg diff`.
//!
//! [`build_plan`] resolves and downloads every dependency declared in
//! `ggg.toml` and computes [`InstallPlan`]s and a [`CleanupPlan`] without
//! writing anything to the project directory.

use std::path::Path;

use anyhow::{Context, Result};

use crate::config::Config;
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
        let used_lock = lock.locked_sha(&dep.name, &dep.git, &dep.rev).is_some();
        let (mut resolved, mut resolve_note) =
            if let Some(sha) = lock.locked_sha(&dep.name, &dep.git, &dep.rev) {
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
                        &resolved.sha[..12], dep.name, e, dep.rev
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

        let cache_dir = dep_cache.entry_path(&resolved);

        let plan = plan_install(&resolved, &cache_dir, project_root, old_state, force)
            .with_context(|| format!("failed to plan install for {:?}", dep.name))?;

        works.push(DepWork { resolved, resolve_note, plan });
    }

    let new_entries: Vec<StateEntry> =
        works.iter().map(|w| w.plan.entry.clone()).collect();

    let cleanup =
        plan_cleanup(old_state, &new_entries, project_root, state_present, force)?;

    Ok(SyncPlan { works, cleanup })
}
