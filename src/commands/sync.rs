//! Implementation of `ggg sync`.
//!
//! Resolves and installs all dependencies declared in `ggg.toml`, downloads
//! the pinned Godot version if not already cached, and removes files left
//! behind by dependencies that have been removed or remapped. Writes
//! `ggg.lock` and `.ggg.state` on every successful run. Supports `--dry-run`
//! (print what would change without writing) and `--force` (overwrite
//! user-modified files).
//!
//! # Flow
//!
//! 1. **Plan phase** - delegates to [`crate::utils::plan::resolve_and_plan`], which
//!    resolves and downloads all dependencies and calls [`plan_install`] /
//!    [`plan_cleanup`] without writing anything to disk.
//! 2. **Check** - if any plan has conflicts, or if `--dry-run` was given,
//!    print a summary of what would happen (and any conflicts) and stop.
//! 3. **Execute phase** - call [`execute_install`] and [`execute_cleanup`],
//!    then write `ggg.lock` and `.ggg.state`.

use std::path::Path;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::dependency::cache::DependencyCache;
use crate::dependency::install::{execute_cleanup, execute_install, CleanupPlan};
use crate::dependency::lockfile::LockFile;
use crate::dependency::state::{LocalState, STATE_FILE};
use crate::godot::cache::GodotCache;
use crate::godot::engine;
use crate::utils::plan::{resolve_and_plan, DepWork, SyncPlan};

use super::init::ensure_gitignore_entry;

pub fn run(dry_run: bool, force: bool) -> Result<()> {
    let project_root = std::env::current_dir()
        .context("failed to determine current directory")?;

    let config = Config::load(Path::new("ggg.toml"))?;
    let mut lock = LockFile::load_or_empty(Path::new("ggg.lock"))?;
    let state_path = project_root.join(STATE_FILE);
    let (old_state, state_present) = LocalState::load_or_empty(&state_path)?;

    let godot_cache = GodotCache::from_env()?;
    engine::ensure(&config.project.godot, &godot_cache)?;

    let dep_cache = DependencyCache::from_env()?;

    let SyncPlan { works, cleanup } =
        resolve_and_plan(&config, &lock, &old_state, state_present, &dep_cache, &project_root, force)?;

    // -------------------------------------------------------------------------
    // Check: any conflicts or --dry-run -> print the plan and stop.
    // -------------------------------------------------------------------------

    let has_conflicts = works.iter().any(|w| !w.plan.conflicts.is_empty())
        || !cleanup.modified.is_empty();

    if dry_run || has_conflicts {
        print_plan(&works, &cleanup);
    }

    if has_conflicts {
        print_conflicts(&works, &cleanup);
        if !dry_run {
            anyhow::bail!("sync blocked: resolve the conflicts above or run with --force");
        }
        return Ok(());
    }

    if dry_run {
        return Ok(());
    }

    // -------------------------------------------------------------------------
    // Execute phase: write files, remove stale entries, persist state.
    // -------------------------------------------------------------------------

    for work in &works {
        execute_install(&work.plan, &project_root)
            .with_context(|| format!("failed to install dependency {:?}", work.resolved.dep.name))?;

        let total   = work.plan.entry.files.len();
        let written = work.plan.to_write.len();
        if written > 0 {
            println!(
                "  {} ({}): installed {} file{} ({} total)",
                work.resolved.dep.name,
                work.resolve_note,
                written,
                if written == 1 { "" } else { "s" },
                total,
            );
        } else {
            println!(
                "  {} ({}): up to date ({} file{})",
                work.resolved.dep.name,
                work.resolve_note,
                total,
                if total == 1 { "" } else { "s" },
            );
        }

        lock.upsert(&work.resolved);
    }

    execute_cleanup(&cleanup, &project_root)?;

    let mut new_state = LocalState::default();
    for work in &works {
        new_state.upsert_entry(work.plan.entry.clone());
    }

    lock.save(Path::new("ggg.lock"))
        .context("failed to write ggg.lock")?;
    new_state
        .save(&state_path)
        .context("failed to write .ggg.state")?;

    ensure_gitignore_entry(Path::new(".gitignore"), STATE_FILE)
        .context("failed to update .gitignore")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

fn print_plan(works: &[DepWork], cleanup: &CleanupPlan) {
    for work in works {
        let total    = work.plan.entry.files.len();
        let to_write = work.plan.to_write.len();
        let name     = &work.resolved.dep.name;
        let note     = &work.resolve_note;

        if !work.plan.conflicts.is_empty() {
            let n = work.plan.conflicts.modified.len() + work.plan.conflicts.unmanaged.len();
            println!(
                "  {} ({}): {} conflict{} (would need --force)",
                name, note, n, if n == 1 { "" } else { "s" },
            );
        } else if to_write > 0 {
            println!(
                "  {} ({}): would install {} file{}",
                name, note, to_write, if to_write == 1 { "" } else { "s" },
            );
        } else {
            println!(
                "  {} ({}): up to date ({} file{})",
                name, note, total, if total == 1 { "" } else { "s" },
            );
        }
    }

    for (_, key) in &cleanup.to_remove {
        println!("  Would remove {}", key);
    }
    for path in &cleanup.modified {
        println!("  Would remove {} (conflicts: modified since last install)", path);
    }
}

fn print_conflicts(works: &[DepWork], cleanup: &CleanupPlan) {
    eprintln!("\nConflicts detected - sync would fail without --force:");

    for work in works {
        if work.plan.conflicts.is_empty() {
            continue;
        }
        eprintln!("\n  {}:", work.resolved.dep.name);
        for f in &work.plan.conflicts.modified {
            eprintln!("    {}  (modified since last install)", f);
        }
        for f in &work.plan.conflicts.unmanaged {
            eprintln!("    {}  (not under ggg's control)", f);
        }
    }

    if !cleanup.modified.is_empty() {
        eprintln!("\n  stale files from removed dependencies:");
        for f in &cleanup.modified {
            eprintln!("    {}  (modified since last install)", f);
        }
    }
}
