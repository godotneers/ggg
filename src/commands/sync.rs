//! Implementation of `ggg sync`.
//!
//! Resolves and installs all dependencies declared in `ggg.toml`, downloads
//! the pinned Godot version if not already cached, and removes files left
//! behind by dependencies that have been removed or remapped. Writes
//! `ggg.lock` and `.ggg.state` on every successful run. Supports `--dry-run`
//! (print what would change without writing) and `--force` (overwrite
//! user-modified files).

use std::path::Path;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::dependency::cache::DependencyCache;
use crate::dependency::download::download_dependency;
use crate::dependency::install::{install, remove_stale, Conflicts, InstallOptions};
use crate::dependency::lockfile::LockFile;
use crate::dependency::resolver::resolve;
use crate::dependency::state::{LocalState, StateEntry, STATE_FILE};
use crate::godot::cache::GodotCache;
use crate::godot::engine;

use super::init::ensure_gitignore_entry;

pub fn run(dry_run: bool, force: bool) -> Result<()> {
    let project_root = std::env::current_dir()
        .context("failed to determine current directory")?;

    let config = Config::load(Path::new("ggg.toml"))?;

    let mut lock = LockFile::load_or_empty(Path::new("ggg.lock"))?;

    // Load the old state for conflict detection and stale-file cleanup.
    let state_path = project_root.join(STATE_FILE);
    let (old_state, state_present) = LocalState::load_or_empty(&state_path)?;

    let options = InstallOptions { dry_run, force };

    // Ensure Godot is downloaded and cached.
    let godot_cache = GodotCache::from_env()?;
    engine::ensure(&config.project.godot, &godot_cache)?;

    let dep_cache = DependencyCache::from_env()?;

    // Build the new state from scratch during this sync so we can diff against
    // the old state for cleanup afterwards.
    let mut new_entries: Vec<StateEntry> = Vec::new();
    // Conflicts accumulated across all deps during a dry-run.
    let mut dry_run_conflicts: Vec<(String, Conflicts)> = Vec::new();

    for dep in &config.dependency {
        // Prefer the locked SHA so that every checkout installs the exact same
        // commit regardless of what the remote branch or tag points at now.
        // Only re-resolve when the dependency changed in ggg.toml (name, URL,
        // or rev), which invalidates the lock entry.
        let used_lock = lock.locked_sha(&dep.name, &dep.git, &dep.rev).is_some();
        let mut resolved = if let Some(sha) = lock.locked_sha(&dep.name, &dep.git, &dep.rev) {
            println!("  {} (locked {})", dep.name, &sha[..12]);
            crate::dependency::ResolvedDependency { dep: dep.clone(), sha: sha.to_owned() }
        } else {
            print!("  Resolving {} ...", dep.name);
            let r = resolve(dep)
                .with_context(|| format!("failed to resolve dependency {:?}", dep.name))?;
            println!(" {}", &r.sha[..12]);
            r
        };

        // Fetch into the dependency cache if not already present.
        if !dep_cache.contains(&resolved) {
            println!("  Downloading {} ...", dep.name);
            let download_result = download_dependency(&resolved);

            // If we used a locked SHA and the download failed, the commit may
            // have been force-pushed away or the lock entry corrupted. Re-resolve
            // from the rev in ggg.toml and retry once before giving up.
            let repo_path = match download_result {
                Err(e) if used_lock => {
                    eprintln!(
                        "  warning: locked commit {} for {:?} is no longer available ({}); \
                         re-resolving from {:?}",
                        &resolved.sha[..12], dep.name, e, dep.rev
                    );
                    print!("  Resolving {} ...", dep.name);
                    resolved = resolve(dep)
                        .with_context(|| format!("failed to re-resolve dependency {:?}", dep.name))?;
                    println!(" {}", &resolved.sha[..12]);
                    download_dependency(&resolved)
                        .with_context(|| format!("failed to download dependency {:?}", dep.name))?
                }
                Err(e) => {
                    return Err(e)
                        .with_context(|| format!("failed to download dependency {:?}", dep.name));
                }
                Ok(path) => path,
            };

            dep_cache
                .install(&resolved, &repo_path)
                .with_context(|| format!("failed to cache dependency {:?}", dep.name))?;

            // Clean up the temporary bare repository.
            let _ = std::fs::remove_dir_all(&repo_path);
        }

        let cache_dir = dep_cache.entry_path(&resolved);

        // Install files from the cache into the project.
        let outcome = install(
            &resolved,
            &cache_dir,
            &project_root,
            &old_state,
            &options,
        )
        .with_context(|| format!("failed to install dependency {:?}", dep.name))?;

        let total = outcome.entry.files.len();

        if dry_run {
            if !outcome.conflicts.is_empty() {
                let n_conflicts = outcome.conflicts.modified.len() + outcome.conflicts.unmanaged.len();
                println!(
                    "  {} - {} conflict{} (would need --force)",
                    dep.name,
                    n_conflicts,
                    if n_conflicts == 1 { "" } else { "s" },
                );
                dry_run_conflicts.push((dep.name.clone(), outcome.conflicts));
            } else if outcome.written > 0 {
                println!(
                    "  Would install {} file{} for {}",
                    outcome.written,
                    if outcome.written == 1 { "" } else { "s" },
                    dep.name,
                );
            } else {
                println!("  {} up to date ({} file{})", dep.name, total, if total == 1 { "" } else { "s" });
            }
        } else {
            if outcome.written > 0 {
                println!(
                    "  Installed {} file{} for {} ({} total)",
                    outcome.written,
                    if outcome.written == 1 { "" } else { "s" },
                    dep.name,
                    total,
                );
            } else {
                println!("  {} up to date ({} file{})", dep.name, total, if total == 1 { "" } else { "s" });
            }
            lock.upsert(&resolved);
        }

        new_entries.push(outcome.entry);
    }

    // Remove files that were installed by a previous sync but are no longer
    // needed (dependency removed, or its map changed).
    remove_stale(&old_state, &new_entries, &project_root, state_present, force, dry_run)?;

    // After processing all deps, report dry-run conflicts and exit with an
    // error so the caller knows a real sync would not succeed without --force.
    if dry_run && !dry_run_conflicts.is_empty() {
        eprintln!("\nConflicts detected - sync would fail without --force:");
        for (name, conflicts) in &dry_run_conflicts {
            eprintln!("\n  {}:", name);
            for f in &conflicts.modified {
                eprintln!("    {f}  (modified since last install)");
            }
            for f in &conflicts.unmanaged {
                eprintln!("    {f}  (not under ggg's control)");
            }
        }
        anyhow::bail!("dry run complete: conflicts must be resolved before syncing");
    }

    if !dry_run {
        let mut new_state = LocalState::default();
        for entry in new_entries {
            new_state.upsert_entry(entry);
        }

        lock.save(Path::new("ggg.lock"))
            .context("failed to write ggg.lock")?;
        new_state
            .save(&state_path)
            .context("failed to write .ggg.state")?;

        // Keep .ggg.state out of git; create .gitignore if it doesn't exist.
        ensure_gitignore_entry(Path::new(".gitignore"), STATE_FILE)
            .context("failed to update .gitignore")?;
    }

    Ok(())
}
