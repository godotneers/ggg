//! Sync planning and execution for all dependencies.
//!
//! [`plan`] resolves and downloads every dependency declared in `ggg.toml` and
//! computes what would change on disk, without writing anything.  [`execute`]
//! carries out a conflict-free plan.
//!
//! Callers are expected to check [`SyncPlan`] for conflicts (or `--dry-run`)
//! between the two calls.
//!
//! # Conflict detection
//!
//! A target file is safe to overwrite when any of the following holds:
//! - The file does not exist yet.
//! - The file is recorded in [`LocalState`] with the same path **and** the
//!   same content hash (GGG-owned, not modified by the user).
//! - The state file was absent when sync started (recovery mode) and the
//!   on-disk hash matches what GGG would install.
//!
//! Any other existing file is a conflict.  Pass `force = true` to skip all
//! conflict checks.
//!
//! # Atomicity
//!
//! Files are staged to a temporary directory inside the project root (same
//! filesystem as the destination), then renamed into place one at a time.  A
//! crash mid-way leaves only partially-installed files whose hashes do not
//! match the state record; the next sync reinstalls them cleanly.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};

use crate::config::{Config, DepKind};
use crate::dependency::cache::DependencyCache;
use crate::dependency::ensure::ensure_dependency;
use crate::dependency::lockfile::LockFile;
use crate::dependency::state::{InstalledFile, LocalState, StateEntry};
use crate::dependency::ResolvedDependency;
use crate::utils::path_key;

/// Metadata filename written into every cache entry; excluded from project
/// installs.
const METADATA_FILE: &str = ".ggg_dep_info.toml";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Everything needed to install one dependency - computed without writing
/// anything to disk.
pub struct InstallPlan {
    /// Every file owned by this dependency (to-write + already up to date).
    pub entry: StateEntry,
    /// Files that need to be written to disk: (cache source, project-relative dest).
    pub to_write: Vec<(PathBuf, PathBuf)>,
    /// Conflicts that block the install.  Empty when `force` is true.
    pub conflicts: Conflicts,
}

/// Conflict information collected during conflict detection.
#[derive(Default)]
pub struct Conflicts {
    /// Files installed by ggg that have since been modified locally.
    pub modified: Vec<String>,
    /// Files that exist on disk but were never installed by ggg.
    pub unmanaged: Vec<String>,
}

impl Conflicts {
    pub fn is_empty(&self) -> bool {
        self.modified.is_empty() && self.unmanaged.is_empty()
    }
}

/// What the cleanup plan found: files safe to delete and files that block
/// deletion because the user modified them.
pub struct CleanupPlan {
    /// Files to remove: (absolute path, display key).
    pub to_remove: Vec<(PathBuf, String)>,
    /// Stale files modified by the user that block removal without `--force`.
    pub modified: Vec<String>,
}

/// One dependency's resolved identity together with its install plan.
pub struct DepWork {
    pub resolved: ResolvedDependency,
    /// Short human-readable note about how the SHA was obtained.
    pub resolve_note: String,
    pub plan: InstallPlan,
}

/// The full plan for a sync run: one [`DepWork`] per dependency plus the
/// stale-file cleanup plan.
pub struct SyncPlan {
    pub works: Vec<DepWork>,
    pub cleanup: CleanupPlan,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve, download (if needed), and plan the install for every dependency
/// in `config`.  Nothing is written to `project_root`.
///
/// Pass `force = true` to suppress conflict detection.
pub fn plan(
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
        let (resolved, resolve_note) = ensure_dependency(dep, lock, dep_cache)?;

        let cache_dir = dep_cache.entry_path(&resolved);

        let force_overwrite = config.sync.as_ref()
            .map(|s| s.force_overwrite.as_slice())
            .unwrap_or(&[]);

        let install_plan = plan_install(&resolved, &cache_dir, project_root, old_state, force, force_overwrite)
            .with_context(|| format!("failed to plan install for {:?}", dep.name))?;

        works.push(DepWork { resolved, resolve_note, plan: install_plan });
    }

    let new_entries: Vec<StateEntry> = works.iter().map(|w| w.plan.entry.clone()).collect();
    let cleanup = plan_cleanup(old_state, &new_entries, project_root, state_present, force)?;

    Ok(SyncPlan { works, cleanup })
}

/// Write files and remove stale entries as described by `sync_plan`.
///
/// Should only be called when `sync_plan` has no conflicts.
pub fn execute(sync_plan: &SyncPlan, project_root: &Path) -> Result<()> {
    for work in &sync_plan.works {
        execute_install(&work.plan, project_root)
            .with_context(|| format!("failed to install {:?}", work.resolved.dep.name))?;
    }
    execute_cleanup(&sync_plan.cleanup, project_root)
}

/// Build a map from project-relative path key to absolute cache path for all
/// files belonging to `dep`.  Used by `ggg diff` to locate the original
/// version of a modified file.
pub fn cache_file_map(
    dep: &ResolvedDependency,
    cache_dir: &Path,
) -> Result<HashMap<String, PathBuf>> {
    let pairs = collect_file_pairs(dep, cache_dir)?;
    Ok(pairs
        .into_iter()
        .map(|(cache_path, proj_path)| (path_key(&proj_path), cache_path))
        .collect())
}

// ---------------------------------------------------------------------------
// Install planning and execution
// ---------------------------------------------------------------------------

fn plan_install(
    dep: &ResolvedDependency,
    cache_dir: &Path,
    project_root: &Path,
    state: &LocalState,
    force: bool,
    force_overwrite: &[String],
) -> Result<InstallPlan> {
    let pairs = collect_file_pairs(dep, cache_dir)?;
    let overwrite_set = build_overwrite_set(force_overwrite)?;

    let conflicts = if force {
        Conflicts::default()
    } else {
        collect_conflicts(&pairs, project_root, state, &overwrite_set)?
    };

    let all_files: Vec<InstalledFile> = pairs
        .iter()
        .map(|(src, rel)| InstalledFile {
            path: path_key(rel),
            hash: hash_file(src).unwrap_or_default(),
        })
        .collect();

    let to_write: Vec<(PathBuf, PathBuf)> = pairs
        .into_iter()
        .filter(|(src, rel_dest)| {
            let dest = project_root.join(rel_dest);
            if !dest.exists() {
                return true;
            }
            match (hash_file(src), hash_file(&dest)) {
                (Ok(sh), Ok(dh)) => sh != dh,
                _ => true,
            }
        })
        .collect();

    Ok(InstallPlan {
        entry: StateEntry { name: dep.dep.name.clone(), files: all_files },
        to_write,
        conflicts,
    })
}

fn execute_install(plan: &InstallPlan, project_root: &Path) -> Result<()> {
    if !plan.to_write.is_empty() {
        stage_and_install(&plan.to_write, project_root)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cleanup planning and execution
// ---------------------------------------------------------------------------

fn plan_cleanup(
    old_state: &LocalState,
    new_entries: &[StateEntry],
    project_root: &Path,
    state_present: bool,
    force: bool,
) -> Result<CleanupPlan> {
    if !state_present {
        if !old_state.entries.is_empty() {
            eprintln!(
                "warning: .ggg.state was not found; stale files from previous \
                 installs cannot be identified and will not be removed. \
                 Run `ggg sync` again to restore the state file."
            );
        }
        return Ok(CleanupPlan { to_remove: vec![], modified: vec![] });
    }

    let new_paths: HashSet<&str> = new_entries
        .iter()
        .flat_map(|e| e.files.iter().map(|f| f.path.as_str()))
        .collect();

    let mut to_remove: Vec<(PathBuf, String)> = Vec::new();
    let mut modified: Vec<String> = Vec::new();

    for entry in &old_state.entries {
        for file in &entry.files {
            if new_paths.contains(file.path.as_str()) {
                continue;
            }
            let abs = project_root
                .join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if !abs.exists() {
                continue;
            }
            let on_disk_hash = hash_file(&abs)
                .with_context(|| format!("failed to hash {}", abs.display()))?;
            if on_disk_hash != file.hash && !force {
                modified.push(file.path.clone());
            } else {
                to_remove.push((abs, file.path.clone()));
            }
        }
    }

    Ok(CleanupPlan { to_remove, modified })
}

fn execute_cleanup(plan: &CleanupPlan, project_root: &Path) -> Result<()> {
    let mut removed_dirs: Vec<PathBuf> = Vec::new();
    for (abs, display) in &plan.to_remove {
        std::fs::remove_file(abs)
            .with_context(|| format!("failed to remove {}", display))?;
        println!("  Removed {}", display);
        if let Some(parent) = abs.parent() {
            removed_dirs.push(parent.to_path_buf());
        }
    }
    prune_empty_dirs(&removed_dirs, project_root);
    Ok(())
}

// ---------------------------------------------------------------------------
// File enumeration
// ---------------------------------------------------------------------------

fn collect_file_pairs(
    dep: &ResolvedDependency,
    cache_dir: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let n_strip = match dep.dep.kind() {
        DepKind::Archive { strip_components, .. } => strip_components,
        DepKind::Git { .. } => dep.dep.strip_components.unwrap_or(0),
        DepKind::AssetLib { .. } => dep.dep.strip_components.unwrap_or(1),
    };

    let mut raw: Vec<(PathBuf, PathBuf)> = Vec::new();
    collect_recursive(cache_dir, Path::new(""), &mut raw)
        .with_context(|| format!("failed to enumerate cache for {:?}", dep.dep.name))?;

    let stripped: Vec<(PathBuf, PathBuf)> = raw
        .into_iter()
        .filter_map(|(abs, rel)| Some((abs, strip_rel_path(&rel, n_strip)?)))
        .collect();

    match &dep.dep.map {
        None => Ok(stripped),
        Some(map_entries) => {
            let mut pairs = Vec::new();
            for entry in map_entries {
                let from = Path::new(&entry.from);
                let to = entry.to.as_deref().map(Path::new).unwrap_or(from);
                let mut matched = false;
                for (abs, virtual_rel) in &stripped {
                    if virtual_rel == from {
                        matched = true;
                        pairs.push((abs.clone(), to.to_path_buf()));
                    } else if let Ok(rest) = virtual_rel.strip_prefix(from) {
                        matched = true;
                        pairs.push((abs.clone(), to.join(rest)));
                    }
                }
                if !matched {
                    anyhow::bail!(
                        "dependency {:?}: map entry `from = {:?}` does not exist in the cached tree",
                        dep.dep.name,
                        entry.from
                    );
                }
            }
            Ok(pairs)
        }
    }
}

fn strip_rel_path(path: &Path, n: u32) -> Option<PathBuf> {
    if n == 0 {
        return Some(path.to_path_buf());
    }
    let comps: Vec<_> = path.components().collect();
    if comps.len() <= n as usize {
        return None;
    }
    Some(comps[n as usize..].iter().collect())
}

fn collect_recursive(
    current: &Path,
    dest_prefix: &Path,
    pairs: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<()> {
    for entry in std::fs::read_dir(current)
        .with_context(|| format!("failed to read directory {}", current.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read entry in {}", current.display()))?;
        let path = entry.path();
        let name = entry.file_name();

        if name == METADATA_FILE {
            continue;
        }

        let child_dest = if dest_prefix == Path::new("") {
            PathBuf::from(&name)
        } else {
            dest_prefix.join(&name)
        };

        if path.is_dir() {
            collect_recursive(&path, &child_dest, pairs)?;
        } else if path.is_file() {
            pairs.push((path, child_dest));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Conflict detection
// ---------------------------------------------------------------------------

fn collect_conflicts(
    pairs: &[(PathBuf, PathBuf)],
    project_root: &Path,
    state: &LocalState,
    overwrite_set: &GlobSet,
) -> Result<Conflicts> {
    let mut conflicts = Conflicts::default();

    for (src, rel_dest) in pairs {
        let dest = project_root.join(rel_dest);
        if !dest.exists() {
            continue;
        }

        let key = path_key(rel_dest);

        if overwrite_set.is_match(&key) {
            continue;
        }

        let on_disk_hash = hash_file(&dest)
            .with_context(|| format!("failed to hash {}", dest.display()))?;
        let would_install = hash_file(src)
            .with_context(|| format!("failed to hash cache file {}", src.display()))?;

        if on_disk_hash == would_install {
            continue;
        }

        if state.is_owned(&key, &on_disk_hash) {
            continue;
        }

        if state.is_managed_path(&key) {
            conflicts.modified.push(key);
        } else {
            conflicts.unmanaged.push(key);
        }
    }

    Ok(conflicts)
}

// ---------------------------------------------------------------------------
// Staging and atomic rename into place
// ---------------------------------------------------------------------------

fn stage_and_install(pairs: &[(PathBuf, PathBuf)], project_root: &Path) -> Result<Vec<InstalledFile>> {
    let tmp = tempfile::Builder::new()
        .prefix(".ggg-install-")
        .tempdir_in(project_root)
        .context("failed to create staging directory in project root")?;

    let mut staged: Vec<(PathBuf, PathBuf, String)> = Vec::new();

    for (src, rel_dest) in pairs {
        let staged_path = tmp.path().join(rel_dest);

        if let Some(parent) = staged_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create staging subdirectory {}", parent.display())
            })?;
        }

        std::fs::copy(src, &staged_path)
            .with_context(|| format!("failed to stage {}", src.display()))?;

        let mut perms = std::fs::metadata(&staged_path)
            .with_context(|| format!("failed to read metadata of {}", staged_path.display()))?
            .permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(&staged_path, perms)
            .with_context(|| format!("failed to set permissions on {}", staged_path.display()))?;

        let hash = hash_file(&staged_path)
            .with_context(|| format!("failed to hash staged file {}", staged_path.display()))?;

        staged.push((staged_path, project_root.join(rel_dest), hash));
    }

    let mut installed = Vec::new();

    for (staged_path, final_path, hash) in &staged {
        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        std::fs::rename(staged_path, final_path)
            .with_context(|| format!("failed to install {}", final_path.display()))?;

        let rel = final_path
            .strip_prefix(project_root)
            .expect("final_path is always under project_root");

        installed.push(InstalledFile { path: path_key(rel), hash: hash.clone() });
    }

    Ok(installed)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_overwrite_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        builder.add(
            Glob::new(p).with_context(|| format!("invalid force_overwrite pattern: {:?}", p))?,
        );
    }
    builder.build().context("failed to build force_overwrite glob set")
}

fn hash_file(path: &Path) -> Result<String> {
    let data = std::fs::read(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut h = Sha256::new();
    h.update(&data);
    Ok(format!("{:x}", h.finalize()))
}

fn prune_empty_dirs(dirs: &[PathBuf], project_root: &Path) {
    let mut unique = dirs.to_vec();
    unique.sort_by(|a, b| b.components().count().cmp(&a.components().count()));
    unique.dedup();

    for start in unique {
        let mut current = start;
        loop {
            if current == project_root {
                break;
            }
            let is_empty = std::fs::read_dir(&current)
                .map(|mut d| d.next().is_none())
                .unwrap_or(false);
            if !is_empty {
                break;
            }
            let _ = std::fs::remove_dir(&current);
            match current.parent().map(|p| p.to_path_buf()) {
                Some(parent) => current = parent,
                None => break,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Dependency, MapEntry};
    use crate::dependency::state::{InstalledFile, StateEntry};
    use tempfile::TempDir;

    // --- helpers ------------------------------------------------------------

    fn make_dep(name: &str, map: Option<Vec<MapEntry>>) -> ResolvedDependency {
        let mut dep = Dependency::new_git(name, "https://example.com/repo.git", "main");
        dep.map = map;
        ResolvedDependency { dep, sha: "a".repeat(40), resolved_url: None, asset_version: None }
    }

    fn make_archive_dep(name: &str, strip: u32, map: Option<Vec<MapEntry>>) -> ResolvedDependency {
        let mut dep = Dependency::new_archive(name, "https://example.com/archive.zip");
        dep.strip_components = if strip == 0 { None } else { Some(strip) };
        dep.map = map;
        ResolvedDependency { dep, sha: "abc123".into(), resolved_url: None, asset_version: None }
    }

    fn write(dir: &Path, rel: &str, content: &[u8]) {
        let abs = dir.join(rel.split('/').collect::<PathBuf>());
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(&abs, content).unwrap();
    }

    fn read(dir: &Path, rel: &str) -> Vec<u8> {
        std::fs::read(dir.join(rel.split('/').collect::<PathBuf>())).unwrap()
    }

    fn exists(dir: &Path, rel: &str) -> bool {
        dir.join(rel.split('/').collect::<PathBuf>()).exists()
    }

    fn is_writable(dir: &Path, rel: &str) -> bool {
        !std::fs::metadata(dir.join(rel.split('/').collect::<PathBuf>()))
            .unwrap()
            .permissions()
            .readonly()
    }

    fn content_hash(data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        format!("{:x}", h.finalize())
    }

    fn state_owns(dep: &str, path: &str, content: &[u8]) -> LocalState {
        let mut state = LocalState::default();
        state.upsert_entry(StateEntry {
            name:  dep.to_string(),
            files: vec![InstalledFile {
                path: path.to_string(),
                hash: content_hash(content),
            }],
        });
        state
    }

    fn inst(
        dep: &ResolvedDependency,
        cache: &Path,
        project: &Path,
        state: &LocalState,
        force: bool,
    ) -> Result<StateEntry> {
        let plan = plan_install(dep, cache, project, state, force, &[])?;
        assert!(plan.conflicts.is_empty(), "unexpected conflicts in inst()");
        execute_install(&plan, project)?;
        Ok(plan.entry)
    }

    // --- file enumeration ---------------------------------------------------

    #[test]
    fn no_map_installs_full_tree() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "addons/gut/gut.gd",     b"# gut");
        write(cache.path(), "addons/gut/sub/util.gd", b"# util");

        let entry = inst(
            &make_dep("gut", None),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        assert_eq!(entry.files.len(), 2);
        assert_eq!(read(project.path(), "addons/gut/gut.gd"),      b"# gut");
        assert_eq!(read(project.path(), "addons/gut/sub/util.gd"), b"# util");
    }

    #[test]
    fn map_from_only_installs_subtree_at_same_path() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "addons/gut/gut.gd",  b"# gut");
        write(cache.path(), "other/ignored.gd",   b"# ignored");

        let map = vec![MapEntry { from: "addons/gut".to_string(), to: None }];
        let entry = inst(
            &make_dep("gut", Some(map)),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        assert_eq!(entry.files.len(), 1);
        assert!(exists(project.path(),  "addons/gut/gut.gd"));
        assert!(!exists(project.path(), "other/ignored.gd"));
    }

    #[test]
    fn map_from_to_installs_at_renamed_destination() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "src/plugin.gd", b"# plugin");

        let map = vec![MapEntry {
            from: "src".to_string(),
            to:   Some("addons/myplugin".to_string()),
        }];
        inst(
            &make_dep("plugin", Some(map)),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        assert!(exists(project.path(),  "addons/myplugin/plugin.gd"));
        assert!(!exists(project.path(), "src/plugin.gd"));
    }

    #[test]
    fn metadata_file_is_excluded_from_install() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "plugin.gd",  b"# plugin");
        write(cache.path(), METADATA_FILE, b"name = 'test'");

        let entry = inst(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        assert_eq!(entry.files.len(), 1);
        assert!(!exists(project.path(), METADATA_FILE));
    }

    #[test]
    fn map_missing_from_path_returns_error() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();

        let map = vec![MapEntry { from: "nonexistent".to_string(), to: None }];
        let result = plan_install(
            &make_dep("dep", Some(map)),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        );

        assert!(result.is_err());
    }

    // --- install correctness ------------------------------------------------

    #[test]
    fn installed_files_are_writable() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "plugin.gd", b"# content");

        let cache_file = cache.path().join("plugin.gd");
        let mut perms = std::fs::metadata(&cache_file).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&cache_file, perms).unwrap();

        inst(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        assert!(is_writable(project.path(), "plugin.gd"));
    }

    #[test]
    fn state_entry_hashes_match_installed_content() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let content = b"# hello";
        write(cache.path(), "plugin.gd", content);

        let entry = inst(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        assert_eq!(entry.files.len(), 1);
        assert_eq!(entry.files[0].path, "plugin.gd");
        assert_eq!(entry.files[0].hash, content_hash(content));
    }

    #[test]
    fn state_entry_paths_use_forward_slashes() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "addons/gut/gut.gd", b"# gut");

        let entry = inst(
            &make_dep("gut", None),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        assert_eq!(entry.files[0].path, "addons/gut/gut.gd");
    }

    // --- conflict detection -------------------------------------------------

    #[test]
    fn no_conflict_when_target_absent() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "plugin.gd", b"# content");

        let plan = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();

        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn no_conflict_when_content_matches_regardless_of_state() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let content = b"# same";
        write(cache.path(),   "plugin.gd", content);
        write(project.path(), "plugin.gd", content);

        let plan = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();

        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn no_conflict_when_ggg_owns_file_and_dep_updated_content() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let old = b"# old";
        let new = b"# new (dep updated)";
        write(cache.path(),   "plugin.gd", new);
        write(project.path(), "plugin.gd", old);
        let state = state_owns("dep", "plugin.gd", old);

        inst(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &state, false,
        ).unwrap();

        assert_eq!(read(project.path(), "plugin.gd"), new);
    }

    #[test]
    fn conflict_when_user_file_exists_with_different_content() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(),   "plugin.gd", b"# dep content");
        write(project.path(), "plugin.gd", b"# user content");

        let plan = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();

        assert!(plan.conflicts.unmanaged.contains(&"plugin.gd".to_string()));
        assert!(plan.conflicts.modified.is_empty());
    }

    #[test]
    fn conflict_message_flags_user_modified_ggg_file() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(),   "plugin.gd", b"# new dep content");
        write(project.path(), "plugin.gd", b"# user modified");
        let state = state_owns("dep", "plugin.gd", b"# original ggg content");

        let plan = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &state, false, &[],
        ).unwrap();

        assert!(plan.conflicts.modified.contains(&"plugin.gd".to_string()));
        assert!(plan.conflicts.unmanaged.is_empty());
    }

    #[test]
    fn force_overwrites_conflicting_user_file() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(),   "plugin.gd", b"# dep content");
        write(project.path(), "plugin.gd", b"# user content");

        inst(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), true,
        ).unwrap();

        assert_eq!(read(project.path(), "plugin.gd"), b"# dep content");
    }

    // --- force_overwrite ----------------------------------------------------

    #[test]
    fn force_overwrite_pattern_bypasses_conflict() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(),   "addons/gut/gut.import", b"# dep import");
        write(project.path(), "addons/gut/gut.import", b"# godot-modified import");

        let patterns = vec!["**/*.import".to_string()];
        let plan = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &patterns,
        ).unwrap();

        assert!(plan.conflicts.is_empty());
        assert_eq!(plan.to_write.len(), 1);

        execute_install(&plan, project.path()).unwrap();
        assert_eq!(read(project.path(), "addons/gut/gut.import"), b"# dep import");
    }

    #[test]
    fn force_overwrite_does_not_affect_non_matching_files() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(),   "plugin.gd",    b"# dep");
        write(cache.path(),   "plugin.import", b"# dep import");
        write(project.path(), "plugin.gd",    b"# user modified");
        write(project.path(), "plugin.import", b"# godot modified");

        let patterns = vec!["**/*.import".to_string()];
        let plan = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &patterns,
        ).unwrap();

        assert!(plan.conflicts.unmanaged.contains(&"plugin.gd".to_string()));
        assert!(plan.conflicts.unmanaged.iter().all(|p| p != "plugin.import"));
    }

    #[test]
    fn force_overwrite_invalid_pattern_returns_error() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "plugin.gd", b"# content");

        let patterns = vec!["[invalid".to_string()];
        let result = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &patterns,
        );

        assert!(result.is_err());
    }

    // --- idempotency --------------------------------------------------------

    #[test]
    fn second_install_writes_nothing_when_content_unchanged() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "addons/gut/gut.gd", b"# gut");
        write(cache.path(), "addons/gut/util.gd", b"# util");

        let first = plan_install(
            &make_dep("gut", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();
        assert_eq!(first.to_write.len(), 2);
        execute_install(&first, project.path()).unwrap();

        let second = plan_install(
            &make_dep("gut", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();
        assert_eq!(second.to_write.len(), 0);
        assert_eq!(second.entry.files.len(), 2);
    }

    #[test]
    fn second_install_writes_only_changed_files() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "a.gd", b"# a");
        write(cache.path(), "b.gd", b"# b");

        let first = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();
        execute_install(&first, project.path()).unwrap();
        let mut state = LocalState::default();
        state.upsert_entry(first.entry);

        write(cache.path(), "b.gd", b"# b updated");

        let second = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &state, false, &[],
        ).unwrap();
        assert_eq!(second.to_write.len(), 1);
        execute_install(&second, project.path()).unwrap();
        assert_eq!(read(project.path(), "b.gd"), b"# b updated");
    }

    // --- plan only (no disk writes) -----------------------------------------

    #[test]
    fn plan_only_writes_no_files() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "plugin.gd", b"# content");

        plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();

        assert!(!exists(project.path(), "plugin.gd"));
    }

    #[test]
    fn plan_returns_correct_entry() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let content = b"# content";
        write(cache.path(), "addons/gut/gut.gd", content);

        let plan = plan_install(
            &make_dep("gut", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();

        assert_eq!(plan.entry.files.len(), 1);
        assert_eq!(plan.entry.files[0].path, "addons/gut/gut.gd");
        assert_eq!(plan.entry.files[0].hash, content_hash(content));
    }

    // --- cleanup ------------------------------------------------------------

    #[test]
    fn stale_file_from_removed_dep_is_deleted() {
        let project = TempDir::new().unwrap();
        let content = b"# gut file";
        write(project.path(), "addons/gut/gut.gd", content);
        let old_state = state_owns("gut", "addons/gut/gut.gd", content);

        let plan = plan_cleanup(&old_state, &[], project.path(), true, false).unwrap();
        execute_cleanup(&plan, project.path()).unwrap();

        assert!(!exists(project.path(), "addons/gut/gut.gd"));
    }

    #[test]
    fn stale_file_from_changed_map_is_deleted() {
        let project = TempDir::new().unwrap();
        let content = b"# file";
        write(project.path(), "old/path/file.gd", content);
        let old_state = state_owns("dep", "old/path/file.gd", content);

        let new_entries = vec![StateEntry {
            name: "dep".to_string(),
            files: vec![InstalledFile {
                path: "new/path/file.gd".to_string(),
                hash: content_hash(content),
            }],
        }];

        let plan = plan_cleanup(&old_state, &new_entries, project.path(), true, false).unwrap();
        execute_cleanup(&plan, project.path()).unwrap();

        assert!(!exists(project.path(), "old/path/file.gd"));
    }

    #[test]
    fn modified_stale_file_is_reported_as_conflict() {
        let project = TempDir::new().unwrap();
        write(project.path(), "plugin.gd", b"# user modified");
        let old_state = state_owns("dep", "plugin.gd", b"# original");

        let plan = plan_cleanup(&old_state, &[], project.path(), true, false).unwrap();

        assert!(plan.modified.contains(&"plugin.gd".to_string()));
        assert!(plan.to_remove.is_empty());
        assert!(exists(project.path(), "plugin.gd"));
    }

    #[test]
    fn force_moves_modified_stale_file_to_remove_list() {
        let project = TempDir::new().unwrap();
        write(project.path(), "plugin.gd", b"# user modified");
        let old_state = state_owns("dep", "plugin.gd", b"# original");

        let plan = plan_cleanup(&old_state, &[], project.path(), true, true).unwrap();

        assert!(plan.modified.is_empty());
        assert_eq!(plan.to_remove.len(), 1);

        execute_cleanup(&plan, project.path()).unwrap();
        assert!(!exists(project.path(), "plugin.gd"));
    }

    #[test]
    fn missing_stale_file_is_silently_skipped() {
        let project  = TempDir::new().unwrap();
        let old_state = state_owns("dep", "plugin.gd", b"# content");

        let plan = plan_cleanup(&old_state, &[], project.path(), true, false).unwrap();
        execute_cleanup(&plan, project.path()).unwrap();
    }

    #[test]
    fn no_cleanup_when_state_was_absent() {
        let project = TempDir::new().unwrap();
        write(project.path(), "plugin.gd", b"# some file");

        let plan = plan_cleanup(
            &LocalState::default(), &[], project.path(),
            false, false,
        ).unwrap();
        execute_cleanup(&plan, project.path()).unwrap();

        assert!(exists(project.path(), "plugin.gd"));
    }

    #[test]
    fn empty_dirs_pruned_after_cleanup() {
        let project = TempDir::new().unwrap();
        let content = b"# file";
        write(project.path(), "addons/gut/gut.gd", content);
        let old_state = state_owns("gut", "addons/gut/gut.gd", content);

        let plan = plan_cleanup(&old_state, &[], project.path(), true, false).unwrap();
        execute_cleanup(&plan, project.path()).unwrap();

        assert!(!exists(project.path(), "addons/gut"));
        assert!(!exists(project.path(), "addons"));
    }

    #[test]
    fn non_empty_dirs_not_pruned_after_cleanup() {
        let project = TempDir::new().unwrap();
        let content = b"# file";
        write(project.path(), "addons/gut/gut.gd",     content);
        write(project.path(), "addons/other/other.gd", b"# other");
        let old_state = state_owns("gut", "addons/gut/gut.gd", content);

        let plan = plan_cleanup(&old_state, &[], project.path(), true, false).unwrap();
        execute_cleanup(&plan, project.path()).unwrap();

        assert!(!exists(project.path(), "addons/gut"));
        assert!(exists(project.path(),  "addons/other/other.gd"));
        assert!(exists(project.path(),  "addons"));
    }

    #[test]
    fn plan_cleanup_writes_nothing() {
        let project = TempDir::new().unwrap();
        let content = b"# file";
        write(project.path(), "plugin.gd", content);
        let old_state = state_owns("dep", "plugin.gd", content);

        plan_cleanup(&old_state, &[], project.path(), true, false).unwrap();

        assert!(exists(project.path(), "plugin.gd"));
    }

    // --- strip_components at install time ------------------------------------

    #[test]
    fn strip_components_one_strips_wrapper_dir() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "wrapper/addons/gut/gut.gd",  b"# gut");
        write(cache.path(), "wrapper/addons/gut/util.gd", b"# util");

        let entry = inst(
            &make_archive_dep("gut", 1, None),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        assert_eq!(entry.files.len(), 2);
        assert!(exists(project.path(), "addons/gut/gut.gd"));
        assert!(exists(project.path(), "addons/gut/util.gd"));
        assert!(!exists(project.path(), "wrapper"));
    }

    #[test]
    fn strip_components_skips_entries_entirely_within_stripped_prefix() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "wrapper/README.md",         b"# readme");
        write(cache.path(), "wrapper/addons/gut/gut.gd", b"# gut");

        inst(
            &make_archive_dep("gut", 1, None),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        assert!(exists(project.path(), "README.md"));
        assert!(exists(project.path(), "addons/gut/gut.gd"));
    }

    #[test]
    fn strip_then_map_uses_post_strip_paths() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "wrapper/addons/gut/gut.gd", b"# gut");
        write(cache.path(), "wrapper/other/ignored.gd",  b"# ignored");

        let map = vec![MapEntry { from: "addons/gut".into(), to: Some("addons/my_gut".into()) }];
        inst(
            &make_archive_dep("gut", 1, Some(map)),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        assert!(exists(project.path(),  "addons/my_gut/gut.gd"));
        assert!(!exists(project.path(), "addons/gut/gut.gd"));
        assert!(!exists(project.path(), "wrapper"));
        assert!(!exists(project.path(), "other/ignored.gd"));
    }

    #[test]
    fn strip_rel_path_zero_is_identity() {
        let p = PathBuf::from("a/b/c.txt");
        assert_eq!(strip_rel_path(&p, 0), Some(p));
    }

    #[test]
    fn strip_rel_path_one() {
        let result = strip_rel_path(Path::new("wrapper/addons/gut.gd"), 1).unwrap();
        assert_eq!(result, PathBuf::from("addons/gut.gd"));
    }

    #[test]
    fn strip_rel_path_returns_none_for_shallow_entries() {
        assert!(strip_rel_path(Path::new("wrapper"), 1).is_none());
    }
}
