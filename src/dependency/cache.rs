//! On-disk cache for installed dependency repositories.
//!
//! Each dependency is stored at:
//!
//! ```text
//! <CACHE_ROOT>/deps/<sha256(normalized_url)>/<commit_sha>/
//! ```
//!
//! where `normalized_url` is the git URL lowercased with its protocol, trailing
//! `.git`, and trailing slashes stripped. Hashing the URL keeps directory names
//! short and filesystem-safe regardless of URL length or special characters.
//!
//! The commit SHA subdirectory means different resolved versions of the same
//! dependency coexist in the cache without interfering with each other.
//!
//! Each entry contains the exported tree files (respecting `.gitattributes`
//! `export-ignore`) plus a `.ggg_dep_info.toml` metadata file.
//!
//! # Cache location
//!
//! Resolved in priority order:
//! 1. `GGG_CACHE_DIR` environment variable
//! 2. Platform default (shared with the Godot binary cache):
//!    - Linux:   `~/.local/share/ggg/deps/`
//!    - macOS:   `~/Library/Application Support/ggg/deps/`
//!    - Windows: `%APPDATA%\ggg\deps\`

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use gix::bstr::ByteSlice;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::DepKind;
use crate::dependency::ResolvedDependency;

/// Filename written into every cache entry for human inspection.
const METADATA_FILE: &str = ".ggg_dep_info.toml";

use crate::cache::resolve_cache_root;

/// Manages the on-disk cache of dependency repository snapshots.
pub struct DependencyCache {
    base: PathBuf,
}

impl DependencyCache {
    /// Create a cache rooted at an explicit path.
    ///
    /// Useful in tests - pass a `tempdir` path to get an isolated cache.
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    /// Resolve the cache location from the environment and create a cache
    /// rooted there.
    ///
    /// Checks `GGG_CACHE_DIR` first, then falls back to the platform default.
    pub fn from_env() -> Result<Self> {
        Ok(Self::new(resolve_cache_root()?.join("deps")))
    }

    /// Returns `true` if this dependency is already present in the cache.
    pub fn contains(&self, dep: &ResolvedDependency) -> bool {
        let dir = self.dep_dir(dep);
        dir.is_dir() && dir.read_dir().map_or(false, |mut d| d.next().is_some())
    }

    /// Install a downloaded dependency artifact into the cache.
    ///
    /// `path` is the value returned by [`super::download::download`]:
    /// - For git deps: a temporary bare clone directory.
    /// - For archive/asset-library deps: a temporary archive file.
    ///
    /// Returns the path to the installed cache entry.
    pub fn install(&self, dep: &ResolvedDependency, path: &Path) -> Result<PathBuf> {
        if self.contains(dep) {
            return Ok(self.dep_dir(dep));
        }
        match dep.dep.kind() {
            DepKind::Git { git, .. } => self.install_git(dep, git, path),
            DepKind::Archive { url, .. } => self.install_archive(dep, url, path),
            DepKind::AssetLib { .. } => {
                let url = dep.resolved_url.as_deref()
                    .expect("AssetLib ResolvedDependency must have resolved_url set");
                self.install_archive(dep, url, path)
            }
        }
    }

    fn install_git(&self, dep: &ResolvedDependency, git: &str, repo_path: &Path) -> Result<PathBuf> {
        let dest = self.dep_dir(dep);
        let hash_dir = self.base.join(url_hash(&normalize_url(git)));

        let tmp_dir = tempfile::Builder::new()
            .prefix(".install-")
            .tempdir_in(&hash_dir)
            .or_else(|_| {
                std::fs::create_dir_all(&hash_dir)
                    .with_context(|| format!("failed to create cache directory {}", hash_dir.display()))?;
                tempfile::Builder::new()
                    .prefix(".install-")
                    .tempdir_in(&hash_dir)
                    .context("failed to create temporary install directory")
            })
            .context("failed to create temporary install directory")?;

        extract_tree(dep, repo_path, tmp_dir.path())
            .with_context(|| format!("failed to extract tree for {:?}", dep.dep.name))?;

        write_git_metadata(dep, tmp_dir.path())
            .context("failed to write dependency metadata")?;

        std::fs::rename(tmp_dir.path(), &dest)
            .with_context(|| format!("failed to move install into cache at {}", dest.display()))?;

        let _ = tmp_dir.keep();
        Ok(dest)
    }

    fn install_archive(&self, dep: &ResolvedDependency, url: &str, archive: &Path) -> Result<PathBuf> {
        let dest = self.dep_dir(dep);

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create cache directory {}", parent.display()))?;
        }
        std::fs::create_dir_all(&dest)
            .with_context(|| format!("failed to create cache entry directory {}", dest.display()))?;

        extract_archive(&dep.dep.name, url, &dep.sha, archive, &dest)
            .with_context(|| format!("failed to extract {:?} into cache", dep.dep.name))?;

        Ok(dest)
    }

    /// The directory where a dependency's cached files are stored.
    ///
    /// The returned path may not exist if the dependency has not been installed
    /// yet.  Use [`contains`](Self::contains) to check first.
    pub fn entry_path(&self, dep: &ResolvedDependency) -> PathBuf {
        self.dep_dir(dep)
    }

    fn dep_dir(&self, dep: &ResolvedDependency) -> PathBuf {
        match dep.dep.kind() {
            DepKind::Git { git, .. } => self.base
                .join(url_hash(&normalize_url(git)))
                .join(&dep.sha),
            DepKind::Archive { url, .. } => self.base
                .join(url_hash(url))
                .join(&dep.sha),
            DepKind::AssetLib { .. } => {
                let url = dep.resolved_url.as_deref()
                    .expect("AssetLib ResolvedDependency must have resolved_url set");
                self.base
                    .join(url_hash(url))
                    .join(&dep.sha)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// URL normalisation and hashing
// ---------------------------------------------------------------------------

/// Normalise a git URL for stable, filesystem-safe hashing.
///
/// - Lowercased
/// - Protocol prefix stripped (`https://`, `git://`, `ssh://`, `git@…:` style)
/// - Trailing `.git` stripped
/// - Trailing slashes stripped
fn normalize_url(url: &str) -> String {
    let mut s = url.to_lowercase();

    // Strip scheme (https://, git://, ssh://, etc.)
    if let Some(pos) = s.find("://") {
        s = s[pos + 3..].to_string();
    } else if let Some(rest) = s.strip_prefix("git@") {
        // git@github.com:user/repo  ->  github.com/user/repo
        s = rest.replacen(':', "/", 1);
    }

    // Strip trailing .git
    if let Some(stripped) = s.strip_suffix(".git") {
        s = stripped.to_string();
    }

    // Strip trailing slashes
    s.trim_end_matches('/').to_string()
}

/// SHA-256 hex digest of the normalised URL (64 lowercase hex characters).
fn url_hash(normalized: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// Archive extraction
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum ArchiveFormat { Zip, TarGz }

fn detect_archive_format(url: &str) -> ArchiveFormat {
    if url.ends_with(".tar.gz") || url.ends_with(".tgz") {
        ArchiveFormat::TarGz
    } else {
        ArchiveFormat::Zip
    }
}

/// Scan `archive` for path traversal, extract into `dest_dir`, and write
/// `.ggg_dep_info.toml`.
fn extract_archive(
    dep_name: &str,
    url: &str,
    archive_sha: &str,
    archive: &Path,
    dest_dir: &Path,
) -> Result<()> {
    use crate::utils::archive as archive_util;

    let fmt = detect_archive_format(url);

    match fmt {
        ArchiveFormat::Zip   => archive_util::scan_zip(archive),
        ArchiveFormat::TarGz => archive_util::scan_tar_gz(archive),
    }
    .with_context(|| format!("archive {dep_name:?} contains unsafe paths - refusing to extract"))?;

    match fmt {
        ArchiveFormat::Zip   => archive_util::extract_zip(archive, dest_dir),
        ArchiveFormat::TarGz => archive_util::extract_tar_gz(archive, dest_dir),
    }
    .with_context(|| format!("failed to extract {dep_name:?}"))?;

    write_archive_metadata(dep_name, url, archive_sha, dest_dir)
        .context("failed to write archive dependency metadata")
}

#[derive(Serialize)]
struct ArchiveDepInfo<'a> {
    name:        &'a str,
    url:         &'a str,
    archive_sha: &'a str,
}

fn write_archive_metadata(name: &str, url: &str, archive_sha: &str, dest_dir: &Path) -> Result<()> {
    let info = ArchiveDepInfo { name, url, archive_sha };
    let content = toml_edit::ser::to_string_pretty(&info)
        .context("failed to serialize archive metadata")?;
    let path = dest_dir.join(METADATA_FILE);
    std::fs::write(&path, &content)
        .with_context(|| format!("failed to write {}", path.display()))
}

// ---------------------------------------------------------------------------
// Tree extraction
// ---------------------------------------------------------------------------

/// Extract the exported tree from the bare repository at `repo_path` into
/// `dest`, skipping any paths with the `export-ignore` gitattribute.
fn extract_tree(dep: &ResolvedDependency, repo_path: &Path, dest: &Path) -> Result<()> {
    let repo = gix::open(repo_path)
        .with_context(|| format!("failed to open bare repository at {}", repo_path.display()))?;

    // Resolve the commit SHA to an object ID.
    let sha_id = gix::ObjectId::from_hex(dep.sha.as_bytes())
        .with_context(|| format!("invalid SHA {:?}", dep.sha))?;

    // Peel the commit to its root tree.
    let tree_id = repo
        .find_object(sha_id)
        .context("commit not found in repository")?
        .peel_to_tree()
        .context("failed to peel commit to tree")?
        .id;

    // Build an in-memory index from the tree - this gives us every file path
    // with its mode and object ID without a separate traversal.
    let index = repo
        .index_from_tree(&tree_id)
        .context("failed to build index from tree")?;

    // Attribute cache that reads .gitattributes from git objects, not the
    // filesystem (correct for a bare repository).
    let mut attr_stack = repo
        .attributes_only(
            &index,
            gix::worktree::stack::state::attributes::Source::IdMapping,
        )
        .context("failed to build attribute cache")?
        .detach();

    // Prime the outcome to track only export-ignore.
    let mut attrs = gix::attrs::search::Outcome::default();
    attrs.initialize_with_selection(
        &gix::attrs::search::MetadataCollection::default(),
        Some("export-ignore"),
    );

    for entry in index.entries() {
        // Skip non-blob entries (submodules, sparse directories).
        if !entry.mode.contains(gix::index::entry::Mode::FILE)
            && !entry.mode.contains(gix::index::entry::Mode::FILE_EXECUTABLE)
            && !entry.mode.contains(gix::index::entry::Mode::SYMLINK)
        {
            continue;
        }

        let path_bytes = entry.path(&index);

        // Check the export-ignore attribute for this path.
        attrs.reset();
        attr_stack
            .at_entry(path_bytes, Some(entry.mode), &repo.objects)
            .with_context(|| format!("failed to get attributes for {:?}", path_bytes.to_str_lossy()))?
            .matching_attributes(&mut attrs);

        if attrs
            .iter_selected()
            .next()
            .expect("initialized with one attr")
            .assignment
            .state
            .is_set()
        {
            continue; // export-ignore is set - skip this file
        }

        write_blob(&repo, entry, path_bytes, dest)?;
    }

    Ok(())
}

/// Read a blob from the repository and write it to `dest_root/<path>`.
///
/// Parent directories are created as needed. The written file is made
/// read-only immediately after writing.
fn write_blob(
    repo: &gix::Repository,
    entry: &gix::index::Entry,
    path_bytes: &gix::bstr::BStr,
    dest_root: &Path,
) -> Result<()> {
    let rel_path = gix::path::from_bstr(path_bytes);
    let dest_path = dest_root.join(&rel_path);

    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let data = repo
        .find_object(entry.id)
        .with_context(|| format!("failed to find blob {}", entry.id))?
        .peel_to_kind(gix::object::Kind::Blob)
        .with_context(|| format!("object {} is not a blob", entry.id))?
        .data
        .to_owned();

    std::fs::write(&dest_path, &data)
        .with_context(|| format!("failed to write {}", dest_path.display()))?;

    // Make the file read-only so the cache cannot be accidentally modified.
    let mut perms = std::fs::metadata(&dest_path)
        .with_context(|| format!("failed to read permissions of {}", dest_path.display()))?
        .permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&dest_path, perms)
        .with_context(|| format!("failed to set read-only on {}", dest_path.display()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Metadata file
// ---------------------------------------------------------------------------

/// Contents of the `.ggg_dep_info.toml` written into a git dep cache entry.
#[derive(Serialize)]
struct GitDepInfo<'a> {
    name: &'a str,
    git:  &'a str,
    rev:  &'a str,
    sha:  &'a str,
}

fn write_git_metadata(dep: &ResolvedDependency, dest: &Path) -> Result<()> {
    let DepKind::Git { git, rev } = dep.dep.kind() else {
        anyhow::bail!("write_git_metadata called on non-git dep");
    };
    let info = GitDepInfo { name: &dep.dep.name, git, rev, sha: &dep.sha };
    let content = toml_edit::ser::to_string_pretty(&info)
        .context("failed to serialize dependency metadata")?;
    let path = dest.join(METADATA_FILE);
    std::fs::write(&path, &content)
        .with_context(|| format!("failed to write {}", path.display()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cache() -> (tempfile::TempDir, DependencyCache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = DependencyCache::new(dir.path().to_path_buf());
        (dir, cache)
    }

    fn resolved(git: &str, sha: &str) -> ResolvedDependency {
        ResolvedDependency {
            dep: crate::config::Dependency::new_git("test", git, "main"),
            sha: sha.into(),
            resolved_url: None,
            asset_version: None,
        }
    }

    // --- URL normalisation ---------------------------------------------------

    #[test]
    fn normalize_strips_https_protocol() {
        assert_eq!(
            normalize_url("https://github.com/foo/bar.git"),
            "github.com/foo/bar"
        );
    }

    #[test]
    fn normalize_strips_ssh_at_syntax() {
        assert_eq!(
            normalize_url("git@github.com:foo/bar.git"),
            "github.com/foo/bar"
        );
    }

    #[test]
    fn normalize_lowercases() {
        assert_eq!(
            normalize_url("https://GITHUB.COM/Foo/Bar.git"),
            "github.com/foo/bar"
        );
    }

    #[test]
    fn normalize_strips_trailing_slashes() {
        assert_eq!(
            normalize_url("https://github.com/foo/bar/"),
            "github.com/foo/bar"
        );
    }

    #[test]
    fn normalize_no_git_suffix_unchanged() {
        assert_eq!(
            normalize_url("https://github.com/foo/bar"),
            "github.com/foo/bar"
        );
    }

    #[test]
    fn same_url_different_protocols_produce_same_hash() {
        let h1 = url_hash(&normalize_url("https://github.com/foo/bar.git"));
        let h2 = url_hash(&normalize_url("git@github.com:foo/bar.git"));
        assert_eq!(h1, h2);
    }

    #[test]
    fn url_hash_is_64_hex_chars() {
        let h = url_hash("github.com/foo/bar");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // --- contains -----------------------------------------------------------

    #[test]
    fn contains_returns_false_for_missing_dep() {
        let (_dir, cache) = make_cache();
        let sha = "a".repeat(40);
        assert!(!cache.contains(&resolved("https://example.com/repo.git", &sha)));
    }

    #[test]
    fn contains_returns_false_for_empty_dir() {
        let (_dir, cache) = make_cache();
        let dep = resolved("https://example.com/repo.git", &"a".repeat(40));
        std::fs::create_dir_all(cache.dep_dir(&dep)).unwrap();
        assert!(!cache.contains(&dep));
    }

    #[test]
    fn dep_dirs_differ_for_different_urls() {
        let (_dir, cache) = make_cache();
        let sha = "a".repeat(40);
        let d1 = cache.dep_dir(&resolved("https://github.com/foo/a.git", &sha));
        let d2 = cache.dep_dir(&resolved("https://github.com/foo/b.git", &sha));
        assert_ne!(d1, d2);
    }

    #[test]
    fn dep_dirs_differ_for_different_shas() {
        let (_dir, cache) = make_cache();
        let d1 = cache.dep_dir(&resolved("https://example.com/repo.git", &"a".repeat(40)));
        let d2 = cache.dep_dir(&resolved("https://example.com/repo.git", &"b".repeat(40)));
        assert_ne!(d1, d2);
    }

    #[test]
    fn from_env_uses_env_var_when_set() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test binary.
        unsafe { std::env::set_var(crate::cache::CACHE_DIR_ENV_VAR, dir.path()); }
        let cache = DependencyCache::from_env().unwrap();
        // from_env appends "deps" to the cache root.
        assert_eq!(cache.base, dir.path().join("deps"));
        unsafe { std::env::remove_var(crate::cache::CACHE_DIR_ENV_VAR); }
    }
}
