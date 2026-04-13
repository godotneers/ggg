//! Installing a cached dependency tree into a Godot project.
//!
//! [`install`] copies files from the dependency cache into the project,
//! respecting the `map` entries in `ggg.toml` and enforcing ownership rules
//! so that user files are never silently overwritten.
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
//! Any other existing file is treated as user-owned and causes the whole
//! install to abort before a single byte is written.  Pass
//! [`InstallOptions::force`] to skip conflict checks.
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

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

use crate::dependency::ResolvedDependency;
use crate::dependency::state::{InstalledFile, LocalState, StateEntry};

/// Metadata filename written into every cache entry; excluded from project
/// installs.
const METADATA_FILE: &str = ".ggg_dep_info.toml";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Options that modify install behaviour.
pub struct InstallOptions {
    /// When `true`, check for conflicts and compute what would be installed,
    /// but write nothing to disk.
    pub dry_run: bool,
    /// When `true`, skip conflict checks and overwrite any existing file.
    pub force: bool,
}

/// Install a dependency from `cache_dir` into `project_root`.
///
/// The outcome of an [`install`] call.
pub struct InstallOutcome {
    /// Every file owned by this dependency (written + already up to date).
    pub entry: StateEntry,
    /// Number of files actually written to disk (0 = everything already matched).
    pub written: usize,
    /// Conflicts that would block a real install.  Non-empty only in dry-run
    /// mode; a normal install bails immediately when conflicts are detected.
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

/// Install a dependency from `cache_dir` into `project_root`.
///
/// In normal mode conflicts cause an immediate error.  In dry-run mode all
/// dependencies are processed and conflicts are returned in
/// [`InstallOutcome::conflicts`] for the caller to report.
///
/// The caller is responsible for persisting the [`StateEntry`] into
/// [`LocalState`].
pub fn install(
    dep: &ResolvedDependency,
    cache_dir: &Path,
    project_root: &Path,
    state: &LocalState,
    options: &InstallOptions,
) -> Result<InstallOutcome> {
    let pairs = collect_file_pairs(dep, cache_dir)?;

    let conflicts = if options.force {
        Conflicts::default()
    } else {
        collect_conflicts(&pairs, project_root, state)?
    };

    // In a real install, bail immediately so nothing is written.
    if !options.dry_run && !conflicts.is_empty() {
        bail!("{}", format_conflict_error(&conflicts));
    }

    // Separate pairs into those that need writing and those already up to date.
    // A file is up to date when its on-disk content already matches the cache.
    let (to_write, _): (Vec<_>, Vec<_>) = pairs.iter().partition(|(src, rel_dest)| {
        let dest = project_root.join(rel_dest);
        if !dest.exists() {
            return true;
        }
        match (hash_file(src), hash_file(&dest)) {
            (Ok(src_hash), Ok(dest_hash)) => src_hash != dest_hash,
            _ => true, // Cannot verify; write to be safe.
        }
    });

    let written = to_write.len();

    // Build the state entry from all pairs using cache hashes as the source of
    // truth. This covers both written files and already-up-to-date files so
    // that stale-cleanup in a future sync can identify the full set.
    let all_files: Vec<InstalledFile> = pairs
        .iter()
        .map(|(src, rel)| InstalledFile {
            path: path_key(rel),
            hash: hash_file(src).unwrap_or_default(),
        })
        .collect();

    if !options.dry_run && !to_write.is_empty() {
        let owned: Vec<(PathBuf, PathBuf)> = to_write.into_iter().cloned().collect();
        stage_and_install(&owned, project_root)?;
    }

    Ok(InstallOutcome {
        entry: StateEntry { name: dep.dep.name.clone(), files: all_files },
        written,
        conflicts,
    })
}

// ---------------------------------------------------------------------------
// File enumeration
// ---------------------------------------------------------------------------

/// Build the list of `(absolute_cache_source, project_relative_destination)`
/// pairs to install, honouring the `map` field in the dependency config.
fn collect_file_pairs(
    dep: &ResolvedDependency,
    cache_dir: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let mut pairs = Vec::new();

    match &dep.dep.map {
        None => {
            // No map: install the full tree at the project root, preserving
            // the directory structure from the cache.
            collect_recursive(cache_dir, Path::new(""), &mut pairs)
                .with_context(|| format!("failed to enumerate cache for {:?}", dep.dep.name))?;
        }
        Some(map_entries) => {
            for entry in map_entries {
                let from = Path::new(&entry.from);
                // `to` defaults to `from` when absent.
                let to = entry.to.as_deref().map(Path::new).unwrap_or(from);
                let src = cache_dir.join(from);

                if !src.exists() {
                    bail!(
                        "dependency {:?}: map entry `from = {:?}` does not exist in the cached tree",
                        dep.dep.name,
                        entry.from
                    );
                }

                if src.is_dir() {
                    collect_recursive(&src, to, &mut pairs).with_context(|| {
                        format!(
                            "failed to enumerate {:?} for dependency {:?}",
                            entry.from, dep.dep.name
                        )
                    })?;
                } else {
                    pairs.push((src, to.to_path_buf()));
                }
            }
        }
    }

    Ok(pairs)
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
fn collect_conflicts(
    pairs: &[(PathBuf, PathBuf)],
    project_root: &Path,
    state: &LocalState,
) -> Result<Conflicts> {
    let mut conflicts = Conflicts::default();

    for (src, rel_dest) in pairs {
        let dest = project_root.join(rel_dest);
        if !dest.exists() {
            continue;
        }

        let key = path_key(rel_dest);
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

/// Format a [`Conflicts`] value into the error string shown to the user.
fn format_conflict_error(c: &Conflicts) -> String {
    let mut msg = String::from("cannot install:");

    if !c.modified.is_empty() {
        msg.push_str("\n\n  The following files were installed by ggg but have been modified locally:");
        for f in &c.modified {
            msg.push_str(&format!("\n    {f}"));
        }
    }

    if !c.unmanaged.is_empty() {
        msg.push_str("\n\n  The following files already exist and are not under ggg's control:");
        for f in &c.unmanaged {
            msg.push_str(&format!("\n    {f}"));
        }
    }

    msg.push_str("\n\nRun with --force to overwrite anyway, or restore the conflicting files first.");
    msg
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

// ---------------------------------------------------------------------------
// Stale file removal
// ---------------------------------------------------------------------------

/// Remove files that the old state records as GGG-owned but that are absent
/// from the new install (dependency removed, or its map changed).
///
/// A file is only deleted when its on-disk hash still matches the recorded
/// hash (i.e. the user has not modified it).  Modified files are warned about
/// and left in place unless `force` is set.
///
/// When `state_present` is `false` the old state was empty because no state
/// file existed, so there is nothing to clean up and the function returns
/// immediately.
pub fn remove_stale(
    old_state: &LocalState,
    new_entries: &[StateEntry],
    project_root: &Path,
    state_present: bool,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    if !state_present {
        if !old_state.entries.is_empty() {
            eprintln!(
                "warning: .ggg.state was not found; stale files from previous \
                 installs cannot be identified and will not be removed. \
                 Run `ggg sync` again to restore the state file."
            );
        }
        return Ok(());
    }

    let new_paths: HashSet<&str> = new_entries
        .iter()
        .flat_map(|e| e.files.iter().map(|f| f.path.as_str()))
        .collect();

    let mut removed_dirs: Vec<PathBuf> = Vec::new();

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
                eprintln!("warning: leaving {} (modified since last install)", file.path);
                continue;
            }

            if dry_run {
                println!("  Would remove {}", file.path);
            } else {
                std::fs::remove_file(&abs)
                    .with_context(|| format!("failed to remove {}", abs.display()))?;
                println!("  Removed {}", file.path);
                if let Some(parent) = abs.parent() {
                    removed_dirs.push(parent.to_path_buf());
                }
            }
        }
    }

    if !dry_run {
        prune_empty_dirs(&removed_dirs, project_root);
    }

    Ok(())
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
        ResolvedDependency {
            dep: Dependency {
                name:  name.to_string(),
                git:   "https://example.com/repo.git".to_string(),
                rev:   "main".to_string(),
                map,
            },
            sha: "a".repeat(40),
        }
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

    fn plain() -> InstallOptions {
        InstallOptions { dry_run: false, force: false }
    }

    /// Thin wrapper that discards the write count - most tests only care about
    /// the returned StateEntry or whether an error occurred.
    fn inst(
        dep: &ResolvedDependency,
        cache: &Path,
        project: &Path,
        state: &LocalState,
        opts: &InstallOptions,
    ) -> Result<StateEntry> {
        super::install(dep, cache, project, state, opts).map(|o| o.entry)
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
            &LocalState::default(), &plain(),
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
            &LocalState::default(), &plain(),
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
            &LocalState::default(), &plain(),
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
            &LocalState::default(), &plain(),
        ).unwrap();

        assert_eq!(entry.files.len(), 1);
        assert!(!exists(project.path(), METADATA_FILE));
    }

    #[test]
    fn map_missing_from_path_returns_error() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();

        let map = vec![MapEntry { from: "nonexistent".to_string(), to: None }];
        let result = inst(
            &make_dep("dep", Some(map)),
            cache.path(), project.path(),
            &LocalState::default(), &plain(),
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
            &LocalState::default(), &plain(),
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
            &LocalState::default(), &plain(),
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
            &LocalState::default(), &plain(),
        ).unwrap();

        assert_eq!(entry.files[0].path, "addons/gut/gut.gd");
    }

    // --- conflict detection -------------------------------------------------

    #[test]
    fn no_conflict_when_target_absent() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "plugin.gd", b"# content");

        inst(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), &plain(),
        ).unwrap();
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

        inst(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), &plain(),
        ).unwrap();
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
            &state, &plain(),
        ).unwrap();

        assert_eq!(read(project.path(), "plugin.gd"), new);
    }

    #[test]
    fn conflict_when_user_file_exists_with_different_content() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(),   "plugin.gd", b"# dep content");
        write(project.path(), "plugin.gd", b"# user content");

        let err = inst(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), &plain(),
        ).unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("plugin.gd"),             "error should name the file");
        assert!(msg.contains("--force"),               "error should suggest --force");
        assert!(msg.contains("not under ggg's control"), "unmanaged file should say so");
    }

    #[test]
    fn conflict_message_flags_user_modified_ggg_file() {
        // GGG installed original_content; user changed it; dep now has new_content.
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(),   "plugin.gd", b"# new dep content");
        write(project.path(), "plugin.gd", b"# user modified");
        let state = state_owns("dep", "plugin.gd", b"# original ggg content");

        let err = inst(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &state, &plain(),
        ).unwrap_err();

        assert!(err.to_string().contains("modified locally"));
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
            &LocalState::default(),
            &InstallOptions { dry_run: false, force: true },
        ).unwrap();

        assert_eq!(read(project.path(), "plugin.gd"), b"# dep content");
    }

    // --- idempotency --------------------------------------------------------

    #[test]
    fn second_install_writes_nothing_when_content_unchanged() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "addons/gut/gut.gd", b"# gut");
        write(cache.path(), "addons/gut/util.gd", b"# util");

        // First install: both files should be written.
        let outcome = super::install(
            &make_dep("gut", None),
            cache.path(), project.path(),
            &LocalState::default(), &plain(),
        ).unwrap();
        assert_eq!(outcome.written, 2);

        // Second install: content is identical, nothing should be written.
        let outcome = super::install(
            &make_dep("gut", None),
            cache.path(), project.path(),
            &LocalState::default(), &plain(),
        ).unwrap();
        assert_eq!(outcome.written, 0);
        // State entry still records all files.
        assert_eq!(outcome.entry.files.len(), 2);
    }

    #[test]
    fn second_install_writes_only_changed_files() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "a.gd", b"# a");
        write(cache.path(), "b.gd", b"# b");

        // First install - capture state so the second install knows ownership.
        let first = super::install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(), &plain(),
        ).unwrap();
        let mut state = LocalState::default();
        state.upsert_entry(first.entry);

        // Simulate dep updating b.gd in the cache.
        write(cache.path(), "b.gd", b"# b updated");

        let second = super::install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &state, &plain(),
        ).unwrap();
        assert_eq!(second.written, 1);
        assert_eq!(read(project.path(), "b.gd"), b"# b updated");
    }

    // --- dry run ------------------------------------------------------------

    #[test]
    fn dry_run_writes_no_files() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(), "plugin.gd", b"# content");

        inst(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(),
            &InstallOptions { dry_run: true, force: false },
        ).unwrap();

        assert!(!exists(project.path(), "plugin.gd"));
    }

    #[test]
    fn dry_run_returns_correct_entry() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        let content = b"# content";
        write(cache.path(), "addons/gut/gut.gd", content);

        let entry = inst(
            &make_dep("gut", None),
            cache.path(), project.path(),
            &LocalState::default(),
            &InstallOptions { dry_run: true, force: false },
        ).unwrap();

        assert_eq!(entry.files.len(), 1);
        assert_eq!(entry.files[0].path, "addons/gut/gut.gd");
        assert_eq!(entry.files[0].hash, content_hash(content));
    }

    #[test]
    fn dry_run_reports_conflicts_without_erroring() {
        // In dry-run mode conflicts are returned in the outcome rather than
        // causing an immediate error, so all deps can be checked in one pass.
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(),   "plugin.gd", b"# dep content");
        write(project.path(), "plugin.gd", b"# user content");

        let outcome = super::install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &LocalState::default(),
            &InstallOptions { dry_run: true, force: false },
        ).unwrap();

        assert!(!outcome.conflicts.unmanaged.is_empty(), "should report unmanaged conflict");
        assert!(outcome.conflicts.modified.is_empty());
        // No files should have been written.
        assert!(!project.path().join("plugin.gd").exists() ||
            std::fs::read(project.path().join("plugin.gd")).unwrap() == b"# user content");
    }

    #[test]
    fn dry_run_reports_modified_conflicts() {
        let cache   = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        write(cache.path(),   "plugin.gd", b"# new dep content");
        write(project.path(), "plugin.gd", b"# user modified");
        let state = state_owns("dep", "plugin.gd", b"# original ggg content");

        let outcome = super::install(
            &make_dep("dep", None),
            cache.path(), project.path(),
            &state,
            &InstallOptions { dry_run: true, force: false },
        ).unwrap();

        assert!(!outcome.conflicts.modified.is_empty(), "should report modified conflict");
        assert!(outcome.conflicts.unmanaged.is_empty());
    }

    // --- stale file removal -------------------------------------------------

    #[test]
    fn stale_file_from_removed_dep_is_deleted() {
        let project = TempDir::new().unwrap();
        let content = b"# gut file";
        write(project.path(), "addons/gut/gut.gd", content);
        let old_state = state_owns("gut", "addons/gut/gut.gd", content);

        remove_stale(&old_state, &[], project.path(), true, false, false).unwrap();

        assert!(!exists(project.path(), "addons/gut/gut.gd"));
    }

    #[test]
    fn stale_file_from_changed_map_is_deleted() {
        let project = TempDir::new().unwrap();
        let content = b"# file";
        write(project.path(), "old/path/file.gd", content);
        let old_state = state_owns("dep", "old/path/file.gd", content);

        // New install puts the same dep's file at a different path.
        let new_entries = vec![StateEntry {
            name:  "dep".to_string(),
            files: vec![InstalledFile {
                path: "new/path/file.gd".to_string(),
                hash: content_hash(content),
            }],
        }];

        remove_stale(&old_state, &new_entries, project.path(), true, false, false).unwrap();

        assert!(!exists(project.path(), "old/path/file.gd"));
    }

    #[test]
    fn modified_stale_file_is_not_deleted_without_force() {
        let project = TempDir::new().unwrap();
        write(project.path(), "plugin.gd", b"# user modified");
        let old_state = state_owns("dep", "plugin.gd", b"# original");

        remove_stale(&old_state, &[], project.path(), true, false, false).unwrap();

        assert!(exists(project.path(), "plugin.gd"));
    }

    #[test]
    fn force_deletes_modified_stale_file() {
        let project = TempDir::new().unwrap();
        write(project.path(), "plugin.gd", b"# user modified");
        let old_state = state_owns("dep", "plugin.gd", b"# original");

        remove_stale(&old_state, &[], project.path(), true, true, false).unwrap();

        assert!(!exists(project.path(), "plugin.gd"));
    }

    #[test]
    fn missing_stale_file_is_silently_skipped() {
        let project  = TempDir::new().unwrap();
        let old_state = state_owns("dep", "plugin.gd", b"# content");

        // File is not on disk - should succeed without error.
        remove_stale(&old_state, &[], project.path(), true, false, false).unwrap();
    }

    #[test]
    fn no_cleanup_when_state_was_absent() {
        let project = TempDir::new().unwrap();
        write(project.path(), "plugin.gd", b"# some file");

        remove_stale(
            &LocalState::default(), &[], project.path(),
            false, // state_present = false
            false, false,
        ).unwrap();

        assert!(exists(project.path(), "plugin.gd"));
    }

    #[test]
    fn empty_dirs_pruned_after_stale_removal() {
        let project = TempDir::new().unwrap();
        let content = b"# file";
        write(project.path(), "addons/gut/gut.gd", content);
        let old_state = state_owns("gut", "addons/gut/gut.gd", content);

        remove_stale(&old_state, &[], project.path(), true, false, false).unwrap();

        assert!(!exists(project.path(), "addons/gut"));
        assert!(!exists(project.path(), "addons"));
    }

    #[test]
    fn non_empty_dirs_not_pruned() {
        let project = TempDir::new().unwrap();
        let content = b"# file";
        write(project.path(), "addons/gut/gut.gd",           content);
        write(project.path(), "addons/other/other.gd",       b"# other");
        let old_state = state_owns("gut", "addons/gut/gut.gd", content);

        remove_stale(&old_state, &[], project.path(), true, false, false).unwrap();

        assert!(!exists(project.path(), "addons/gut"));
        assert!(exists(project.path(),  "addons/other/other.gd"));
        assert!(exists(project.path(),  "addons")); // still has other/
    }

    #[test]
    fn dry_run_stale_removal_writes_nothing() {
        let project = TempDir::new().unwrap();
        let content = b"# file";
        write(project.path(), "plugin.gd", content);
        let old_state = state_owns("dep", "plugin.gd", content);

        remove_stale(&old_state, &[], project.path(), true, false, true).unwrap();

        assert!(exists(project.path(), "plugin.gd"));
    }
}
