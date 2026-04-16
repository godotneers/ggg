//! Implementation of `ggg diff`.
//!
//! Shows a unified diff between the version of a file installed by ggg and
//! the version currently on disk, for every ggg-owned file that has been
//! modified locally.
//!
//! Exits with code 1 when modified files are found (so scripts and CI can
//! detect the situation), or 0 when every ggg-owned file is unmodified.

use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::dependency::cache::DependencyCache;
use crate::dependency::install::cache_file_map;
use crate::dependency::lockfile::LockFile;
use crate::dependency::state::{LocalState, STATE_FILE};
use crate::utils::plan::{resolve_and_plan, SyncPlan};

pub fn run(file: Option<&str>) -> Result<()> {
    let project_root = std::env::current_dir()
        .context("failed to determine current directory")?;

    let config = Config::load(Path::new("ggg.toml"))?;
    let lock = LockFile::load_or_empty(Path::new("ggg.lock"))?;
    let state_path = project_root.join(STATE_FILE);
    let (old_state, state_present) = LocalState::load_or_empty(&state_path)?;
    let dep_cache = DependencyCache::from_env()?;

    // Normalise the optional path filter to forward slashes so it matches the
    // keys stored in LocalState regardless of what the user typed.
    let filter: Option<String> = file.map(|f| f.replace('\\', "/"));

    let SyncPlan { works, .. } =
        resolve_and_plan(&config, &lock, &old_state, state_present, &dep_cache, &project_root, false)?;

    let use_color = std::io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none();

    let fmt = if use_color {
        diffy::PatchFormatter::new().with_color()
    } else {
        diffy::PatchFormatter::new()
    };

    let mut any_printed = false;

    for work in &works {
        // plan_install already identified which owned files were modified.
        let modified: Vec<&str> = work
            .plan
            .conflicts
            .modified
            .iter()
            .filter(|p| filter.as_deref().map_or(true, |f| p.as_str() == f))
            .map(|p| p.as_str())
            .collect();

        if modified.is_empty() {
            continue;
        }

        let cache_dir = dep_cache.entry_path(&work.resolved);
        let file_map = cache_file_map(&work.resolved, &cache_dir)
            .with_context(|| {
                format!("failed to enumerate cache for {:?}", work.resolved.dep.name)
            })?;

        if any_printed {
            println!();
        }
        println!("Diff for {} ({}):", work.resolved.dep.name, &work.resolve_note);

        for path in modified {
            let Some(cache_path) = file_map.get(path) else {
                eprintln!(
                    "  warning: {} not found in cache for {:?}",
                    path, work.resolved.dep.name
                );
                continue;
            };

            let abs = project_root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));

            let cache_content = match std::fs::read_to_string(cache_path) {
                Ok(s) => s,
                Err(_) => {
                    println!("\n  {} (binary file, skipping)", path);
                    any_printed = true;
                    continue;
                }
            };
            let disk_content = match std::fs::read_to_string(&abs) {
                Ok(s) => s,
                Err(_) => {
                    println!("\n  {} (binary file, skipping)", path);
                    any_printed = true;
                    continue;
                }
            };

            println!("\n  {}", path);
            println!();

            let patch = diffy::create_patch(&cache_content, &disk_content);
            print!("{}", fmt.fmt_patch(&patch));
            any_printed = true;
        }
    }

    if !any_printed {
        eprintln!("no modified files");
        return Ok(());
    }

    std::process::exit(1);
}
