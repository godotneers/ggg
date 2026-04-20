//! Implementation of `ggg ls-dep`.
//!
//! Lists the raw contents of a dependency's cache entry so the user can
//! determine the right `strip_components` value and `map` entries before
//! running `ggg sync`.
//!
//! Shows the cache tree **before** `strip_components` or `map` are applied,
//! because that is the level at which those values need to be reasoned about.
//!
//! If the dependency is already in the cache a network fetch is skipped.
//! When a fetch is needed the lock file is updated as a side effect.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::{Config, DepKind};
use crate::dependency::cache::DependencyCache;
use crate::dependency::lockfile::LockFile;
use crate::dependency::ensure::ensure_dependency;
use crate::utils::path_key;

const METADATA_FILE: &str = ".ggg_dep_info.toml";

pub fn run(name: &str, show_all: bool) -> Result<()> {
    let config = Config::load(Path::new("ggg.toml"))?;

    let dep = config.get_dependency(name)
        .with_context(|| format!("dependency {:?} not found in ggg.toml", name))?;

    let mut lock = LockFile::load_or_empty(Path::new("ggg.lock"))?;
    let dep_cache = DependencyCache::from_env()?;

    let (resolved, _note) = ensure_dependency(dep, &lock, &dep_cache)
        .with_context(|| format!("failed to resolve {:?}", name))?;

    lock.upsert(&resolved);
    lock.save(Path::new("ggg.lock")).context("failed to write ggg.lock")?;

    let cache_dir = dep_cache.entry_path(&resolved);

    let mut files: Vec<String> = Vec::new();
    collect_files(&cache_dir, Path::new(""), &mut files)
        .with_context(|| format!("failed to enumerate cache for {:?}", name))?;
    files.sort_unstable();

    // Header: "name  (rev -> sha[:8]...)" for git, "name  (sha[:8]...)" for archive.
    let version_note = match dep.kind() {
        DepKind::Git { rev, .. }    => format!("{} -> {}...", rev, &resolved.sha[..8]),
        DepKind::Archive { .. }     => format!("{}...", &resolved.sha[..8]),
        DepKind::AssetLib { asset_id } => {
            let version = resolved.asset_version
                .map(|v| format!("v{} ", v))
                .unwrap_or_default();
            format!("asset #{asset_id} {version}-> {}...", &resolved.sha[..8])
        }
    };
    println!("{name}  ({version_note})");

    if show_all {
        for f in &files {
            println!("{f}");
        }
    } else {
        let root = build_tree(&files);
        print_node(&root, 0);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// File enumeration
// ---------------------------------------------------------------------------

fn collect_files(dir: &Path, prefix: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let name = entry.file_name();

        if name == METADATA_FILE {
            continue;
        }

        let child: PathBuf = if prefix == Path::new("") {
            PathBuf::from(&name)
        } else {
            prefix.join(&name)
        };

        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, &child, out)?;
        } else if path.is_file() {
            out.push(path_key(&child));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Collapsed tree view
// ---------------------------------------------------------------------------

struct DirNode {
    /// Direct files in this directory (bare names, not full paths).
    files: Vec<String>,
    /// Subdirectories, keyed by name; BTreeMap keeps them sorted.
    subdirs: BTreeMap<String, DirNode>,
}

impl DirNode {
    fn new() -> Self {
        Self { files: Vec::new(), subdirs: BTreeMap::new() }
    }

    fn insert(&mut self, parts: &[&str]) {
        match parts {
            [] => {}
            [file] => self.files.push((*file).to_string()),
            [dir, rest @ ..] => {
                self.subdirs
                    .entry((*dir).to_string())
                    .or_insert_with(DirNode::new)
                    .insert(rest);
            }
        }
    }
}

fn build_tree(files: &[String]) -> DirNode {
    let mut root = DirNode::new();
    for f in files {
        let parts: Vec<&str> = f.split('/').collect();
        root.insert(&parts);
    }
    root
}

fn print_node(node: &DirNode, depth: usize) {
    let indent = "  ".repeat(depth);

    // Directories first (BTreeMap is already sorted), then files.
    for (name, subdir) in &node.subdirs {
        let n = subdir.files.len();
        if n > 0 {
            println!("{indent}{name}/  {} file{}", n, if n == 1 { "" } else { "s" });
        } else {
            println!("{indent}{name}/");
        }
        print_node(subdir, depth + 1);
    }

}
