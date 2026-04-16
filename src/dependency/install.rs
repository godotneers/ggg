//! Installing a cached dependency tree into a Godot project.
//!
//! [`plan_install`] computes what would be installed (files to write plus any
//! conflicts) without touching disk.  [`execute_install`] carries out a plan
//! that has already been checked for conflicts.
//!
//! [`plan_cleanup`] computes which stale files (left by a removed or remapped
//! dependency) would be deleted, separating unmodified files (safe to delete)
//! from user-modified ones (conflicts).  [`execute_cleanup`] carries out the
//! deletion.
//!
//! `sync.rs` always runs the plan phase across all dependencies first, and
//! only calls the execute functions when every plan is conflict-free.
//!
//! # Conflict detection
//!
//! A target file is safe to overwrite when any of the following holds:
//! - The file does not exist yet.
//! - The file is recorded in [`LocalState`] with the same path **and** the
//!   same content hash (GGG-owned, not modified by the user).
//! - The state file was absent when sync started (recovery mode) and the
//!   on-disk hash matches what GGG would install - indicating a prior install
//!   whose state record was lost.
//!
//! Any other existing file is a conflict.  Pass `force = true` to skip all
//! conflict checks.
//!
//! # Atomicity
//!
//! Files are staged to a temporary directory created inside the project root
//! (ensuring the same filesystem as the destination), then renamed into place.
//! A rename of a single file is atomic on all major operating systems.  The
//! sequence of renames is not atomic, but a crash mid-way leaves only
//! partially-installed files whose hashes do not match the state record; the
//! next sync detects this via the ownership check and reinstalls cleanly.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};

use crate::config::DepKind;
use crate::dependency::ResolvedDependency;
use crate::dependency::state::{InstalledFile, LocalState, StateEntry};

/// Metadata filename written into every cache entry; excluded from project
/// installs.
const METADATA_FILE: &str = ".ggg_dep_info.toml";

// ---------------------------------------------------------------------------
// Public API
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

/// What [`plan_cleanup`] found: files safe to delete and files that block
/// deletion because the user modified them.
pub struct CleanupPlan {
    /// Files to remove: (absolute path, display key).
    pub to_remove: Vec<(PathBuf, String)>,
    /// Stale files modified by the user that block removal without `--force`.
    pub modified: Vec<String>,
}

/// Compute what would be installed for `dep` without writing anything to disk.
///
/// Set `force` to skip all conflict checks.  `force_overwrite` is a list of
/// glob patterns (e.g. `**/*.import`) matched against project-relative paths;
/// files that match bypass conflict detection and are always overwritten.
pub fn plan_install(
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

    // Build the state entry using cache hashes as the source of truth.
    // This covers both to-write and already-up-to-date files so that a future
    // cleanup can identify the full set owned by this dependency.
    let all_files: Vec<InstalledFile> = pairs
        .iter()
        .map(|(src, rel)| InstalledFile {
            path: path_key(rel),
            hash: hash_file(src).unwrap_or_default(),
        })
        .collect();

    // Determine which files actually need writing.
    let to_write: Vec<(PathBuf, PathBuf)> = pairs
        .into_iter()
        .filter(|(src, rel_dest)| {
            let dest = project_root.join(rel_dest);
            if !dest.exists() {
                return true;
            }
            match (hash_file(src), hash_file(&dest)) {
                (Ok(sh), Ok(dh)) => sh != dh,
                _ => true, // Cannot verify; write to be safe.
            }
        })
        .collect();

    Ok(InstallPlan {
        entry: StateEntry { name: dep.dep.name.clone(), files: all_files },
        to_write,
        conflicts,
    })
}

/// Write the files described by `plan` to disk.
///
/// Should only be called when `plan.conflicts` is empty.
pub fn execute_install(plan: &InstallPlan, project_root: &Path) -> Result<()> {
    if !plan.to_write.is_empty() {
        stage_and_install(&plan.to_write, project_root)?;
    }
    Ok(())
}

/// Compute which stale files should be removed and which are blocked by local
/// modifications.
///
/// A stale file is one that the old state records as GGG-owned but that is
/// absent from the new set of entries (dependency removed or map changed).
///
/// When `state_present` is `false` there is no old state to clean up from and
/// the function returns an empty plan (printing a warning if needed).
///
/// Set `force` to move modified stale files into `to_remove` instead of
/// `modified`.
pub fn plan_cleanup(
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

/// Delete the stale files listed in `plan` and prune empty directories.
pub fn execute_cleanup(plan: &CleanupPlan, project_root: &Path) -> Result<()> {
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

/// Build the list of `(absolute_cache_source, project_relative_destination)`
/// pairs to install, honouring the `strip_components` and `map` fields in the
/// dependency config.
///
/// For archive dependencies `strip_components` is applied first (on the
/// cache-relative virtual paths), then the `map` entries are matched against
/// the post-strip paths.  For git dependencies `strip_components` is always 0.
fn collect_file_pairs(
    dep: &ResolvedDependency,
    cache_dir: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let n_strip = match dep.dep.kind() {
        DepKind::Archive { strip_components, .. } => strip_components,
        DepKind::Git { .. } => dep.dep.strip_components.unwrap_or(0),
        // Asset library archives always wrap content in a root folder; strip
        // it by default.  The user can override with strip_components = 0 in
        // ggg.toml if needed.
        DepKind::AssetLib { .. } => dep.dep.strip_components.unwrap_or(1),
    };

    // Collect all files from the cache with their cache-relative paths.
    let mut raw: Vec<(PathBuf, PathBuf)> = Vec::new();
    collect_recursive(cache_dir, Path::new(""), &mut raw)
        .with_context(|| format!("failed to enumerate cache for {:?}", dep.dep.name))?;

    // Apply strip_components to produce the virtual (post-strip) relative paths.
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
                // `to` defaults to `from` when absent.
                let to = entry.to.as_deref().map(Path::new).unwrap_or(from);

                let mut matched = false;
                for (abs, virtual_rel) in &stripped {
                    if virtual_rel == from {
                        // Exact file match.
                        matched = true;
                        pairs.push((abs.clone(), to.to_path_buf()));
                    } else if let Ok(rest) = virtual_rel.strip_prefix(from) {
                        // `from` is a directory prefix; keep the remainder
                        // under `to`.
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

/// Strip `n` leading path components from `path`. Returns `None` if the path
/// has `n` or fewer components (the entry is entirely within the stripped
/// prefix and should be skipped).
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

/// Recursively enumerate files under `current`, recording them in `pairs` as
/// `(absolute_path, dest_prefix/<file_relative_to_current>)`.
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

        // Build the destination path for this entry.
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

/// Collect all conflicts without bailing - separates ggg-modified files from
/// unmanaged files so callers can report them with appropriate messages.
///
/// Files whose project-relative path key matches `overwrite_set` are skipped:
/// they will be unconditionally overwritten regardless of their on-disk state.
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

        // If the on-disk content is already identical to what we would write,
        // overwriting changes nothing - safe regardless of what the state says.
        let would_install = hash_file(src)
            .with_context(|| format!("failed to hash cache file {}", src.display()))?;
        if on_disk_hash == would_install {
            continue;
        }

        // Content differs. Safe only if ggg owns the file and it is unmodified.
        if state.is_owned(&key, &on_disk_hash) {
            continue; // GGG-owned and unmodified; dep updated its content.
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

fn stage_and_install(
    pairs: &[(PathBuf, PathBuf)],
    project_root: &Path,
) -> Result<Vec<InstalledFile>> {
    // The staging directory lives inside the project root so that it is on the
    // same filesystem as the destination, making renames atomic.
    let tmp = tempfile::Builder::new()
        .prefix(".ggg-install-")
        .tempdir_in(project_root)
        .context("failed to create staging directory in project root")?;

    // Copy all files into the staging area and compute their hashes.
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

        // Project-side copies should be writable; only the cache copies are
        // kept read-only.
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

    // All copies succeeded - rename each staged file to its final location.
    let mut installed = Vec::new();

    for (staged_path, final_path, hash) in &staged {
        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create directory {}", parent.display())
            })?;
        }

        std::fs::rename(staged_path, final_path)
            .with_context(|| format!("failed to install {}", final_path.display()))?;

        let rel = final_path
            .strip_prefix(project_root)
            .expect("final_path is always under project_root");

        installed.push(InstalledFile {
            path: path_key(rel),
            hash: hash.clone(),
        });
    }

    // TempDir::drop removes the staging directory (now containing only empty
    // subdirectories) via remove_dir_all.

    Ok(installed)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a [`GlobSet`] from the `force_overwrite` pattern list.
///
/// Returns an error if any pattern is syntactically invalid.  An empty
/// pattern list produces an empty set that matches nothing.
fn build_overwrite_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        builder.add(
            Glob::new(p)
                .with_context(|| format!("invalid force_overwrite pattern: {:?}", p))?,
        );
    }
    builder.build().context("failed to build force_overwrite glob set")
}

/// Build a map from project-relative path key to absolute cache path for all
/// files belonging to `dep`.  Used by `ggg diff` to locate the original
/// version of a modified file.
pub fn cache_file_map(
    dep: &ResolvedDependency,
    cache_dir: &Path,
) -> Result<std::collections::HashMap<String, PathBuf>> {
    let pairs = collect_file_pairs(dep, cache_dir)?;
    Ok(pairs
        .into_iter()
        .map(|(cache_path, proj_path)| (path_key(&proj_path), cache_path))
        .collect())
}

/// Hash the contents of `path` and return the lowercase hex SHA-256 digest.
pub fn hash_file(path: &Path) -> Result<String> {
    let data = std::fs::read(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut h = Sha256::new();
    h.update(&data);
    Ok(format!("{:x}", h.finalize()))
}

/// Convert a relative [`Path`] to a forward-slash string for cross-platform
/// consistency when stored in `.ggg.state`.
fn path_key(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Remove directories that became empty after stale file deletion, walking
/// upward from each affected directory.  Stops before the project root.
/// Errors are ignored since a leftover empty directory is harmless.
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

    /// Write `content` to `<dir>/<rel>`, creating parents as needed.
    /// `rel` always uses forward slashes.
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

    /// Build a LocalState where `dep` owns `path` with the hash of `content`.
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

    /// Plan and execute an install, returning the state entry.
    /// Panics if the plan has conflicts.
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

        // Mark the cache copy read-only, as the real cache does.
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
        // File exists on disk with the same bytes we'd install (e.g. asset
        // library migration) - safe even without a state entry.
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
        // State records the old hash; dep now has new content.  GGG owns the
        // file so it may be overwritten.
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

        assert!(plan.conflicts.unmanaged.contains(&"plugin.gd".to_string()),
            "should report unmanaged conflict");
        assert!(plan.conflicts.modified.is_empty());
    }

    #[test]
    fn conflict_message_flags_user_modified_ggg_file() {
        // GGG installed original_content; user changed it; dep now has new_content.
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

        assert!(plan.conflicts.modified.contains(&"plugin.gd".to_string()),
            "should report modified conflict");
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
        // A file is on disk with different content than the cache, and is NOT
        // recorded in the state (would normally be an unmanaged conflict).
        // With a matching force_overwrite pattern it must not be reported as a
        // conflict and must be overwritten on execute.
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

        assert!(plan.conflicts.is_empty(), "force_overwrite file should not be a conflict");
        assert_eq!(plan.to_write.len(), 1, "file with differing content should still be in to_write");

        execute_install(&plan, project.path()).unwrap();
        assert_eq!(read(project.path(), "addons/gut/gut.import"), b"# dep import");
    }

    #[test]
    fn force_overwrite_does_not_affect_non_matching_files() {
        // A non-matching file next to a matching one should still be conflict-checked.
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(),   "plugin.gd",     b"# dep");
        write(cache.path(),   "plugin.import",  b"# dep import");
        write(project.path(), "plugin.gd",     b"# user modified");
        write(project.path(), "plugin.import",  b"# godot modified");

        let patterns = vec!["**/*.import".to_string()];
        let plan = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &patterns,
        ).unwrap();

        assert!(plan.conflicts.unmanaged.contains(&"plugin.gd".to_string()),
            "non-matching file should still be reported as conflict");
        assert!(plan.conflicts.unmanaged.iter().all(|p| p != "plugin.import"),
            "matching file should not be reported as conflict");
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

        assert!(result.is_err(), "invalid glob pattern should return an error");
    }

    // --- idempotency --------------------------------------------------------

    #[test]
    fn second_install_writes_nothing_when_content_unchanged() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "addons/gut/gut.gd", b"# gut");
        write(cache.path(), "addons/gut/util.gd", b"# util");

        // First install: both files should be written.
        let first = plan_install(
            &make_dep("gut", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();
        assert_eq!(first.to_write.len(), 2);
        execute_install(&first, project.path()).unwrap();

        // Second install: content is identical, nothing should be written.
        let second = plan_install(
            &make_dep("gut", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();
        assert_eq!(second.to_write.len(), 0);
        // State entry still records all files.
        assert_eq!(second.entry.files.len(), 2);
    }

    #[test]
    fn second_install_writes_only_changed_files() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "a.gd", b"# a");
        write(cache.path(), "b.gd", b"# b");

        // First install - capture state so the second install knows ownership.
        let first = plan_install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), false, &[],
        ).unwrap();
        execute_install(&first, project.path()).unwrap();
        let mut state = LocalState::default();
        state.upsert_entry(first.entry);

        // Simulate dep updating b.gd in the cache.
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

        assert!(plan.modified.contains(&"plugin.gd".to_string()),
            "modified stale file should be reported as a conflict");
        assert!(plan.to_remove.is_empty());
        // File must still be on disk - we did not execute.
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

        // File is not on disk - plan should be empty, execute should succeed.
        let plan = plan_cleanup(&old_state, &[], project.path(), true, false).unwrap();
        execute_cleanup(&plan, project.path()).unwrap();
    }

    #[test]
    fn no_cleanup_when_state_was_absent() {
        let project = TempDir::new().unwrap();
        write(project.path(), "plugin.gd", b"# some file");

        let plan = plan_cleanup(
            &LocalState::default(), &[], project.path(),
            false, // state_present = false
            false,
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
        write(project.path(), "addons/gut/gut.gd",           content);
        write(project.path(), "addons/other/other.gd",       b"# other");
        let old_state = state_owns("gut", "addons/gut/gut.gd", content);

        let plan = plan_cleanup(&old_state, &[], project.path(), true, false).unwrap();
        execute_cleanup(&plan, project.path()).unwrap();

        assert!(!exists(project.path(), "addons/gut"));
        assert!(exists(project.path(),  "addons/other/other.gd"));
        assert!(exists(project.path(),  "addons")); // still has other/
    }

    #[test]
    fn plan_cleanup_writes_nothing() {
        let project = TempDir::new().unwrap();
        let content = b"# file";
        write(project.path(), "plugin.gd", content);
        let old_state = state_owns("dep", "plugin.gd", content);

        // plan without execute must not touch disk.
        plan_cleanup(&old_state, &[], project.path(), true, false).unwrap();

        assert!(exists(project.path(), "plugin.gd"));
    }

    // --- strip_components at install time ------------------------------------

    #[test]
    fn strip_components_one_strips_wrapper_dir() {
        // Archive cache has a wrapper directory; strip_components = 1 should
        // remove it so files land directly at the post-strip paths.
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
        // A file living exactly at the stripped prefix level (e.g. a top-level
        // README inside the wrapper) should be silently skipped.
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "wrapper/README.md",         b"# readme");
        write(cache.path(), "wrapper/addons/gut/gut.gd", b"# gut");

        inst(
            &make_archive_dep("gut", 1, None),
            cache.path(), project.path(),
            &LocalState::default(), false,
        ).unwrap();

        // README.md is at depth 1 inside the wrapper; after stripping it
        // becomes "README.md" and should be installed.
        assert!(exists(project.path(), "README.md"));
        assert!(exists(project.path(), "addons/gut/gut.gd"));
    }

    #[test]
    fn strip_then_map_uses_post_strip_paths() {
        // strip_components = 1 removes "wrapper/"; the map `from` is written
        // in terms of post-strip paths.
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
