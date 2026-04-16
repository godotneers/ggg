//! Committed lock file (`ggg.lock`) recording the resolved commit SHA for each
//! dependency.
//!
//! This is the reproducibility record: it is committed to git alongside the
//! project so that any checkout can re-install the exact same dependency
//! versions without network resolution.
//!
//! Local install tracking (which files were written into this working tree) is
//! stored separately in `.ggg.state`, which is gitignored.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::DepKind;
use crate::dependency::ResolvedDependency;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Contents of `ggg.lock`.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct LockFile {
    /// One entry per dependency, in the order they appear in `ggg.toml`.
    #[serde(default, rename = "dependency")]
    pub entries: Vec<LockEntry>,
}

/// One dependency record inside `ggg.lock`.
///
/// Git entries have `git`, `rev`, and `sha` set.
/// Archive entries have `url` and `archive_sha` set.
/// Asset library entries have `asset_id`, `asset_version`, `url`, and
/// `archive_sha` set.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LockEntry {
    /// Matches the `name` field in `ggg.toml`.
    pub name: String,

    // --- git dep fields ---
    /// The git URL from `ggg.toml`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    /// The `rev` from `ggg.toml`. Together with `name` and `git` forms the
    /// lock key: any change forces a fresh resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    /// Resolved 40-character lowercase hex commit SHA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,

    // --- archive dep fields ---
    /// The archive URL from `ggg.toml` (archive deps) or resolved from the
    /// asset library API (asset lib deps).  Together with `name` forms the
    /// lock key for archive deps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// SHA-256 hex digest of the downloaded archive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_sha: Option<String>,

    // --- asset library dep fields ---
    /// Numeric asset ID from the Godot Asset Library. Together with `name`
    /// forms the lock key for asset lib deps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<u32>,
    /// Asset library version integer at the time the lock was written.
    /// Used by `ggg update` to detect whether a newer version is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_version: Option<u32>,
}

// ---------------------------------------------------------------------------
// impl LockFile
// ---------------------------------------------------------------------------

impl LockFile {
    /// Load `ggg.lock` from `path`.  Returns an empty lock file if the path
    /// does not exist (first-time sync).
    pub fn load_or_empty(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml_edit::de::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Serialise and write `ggg.lock` to `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml_edit::ser::to_string_pretty(self)
            .context("failed to serialise lock file")?;
        std::fs::write(path, content)
            .with_context(|| format!("failed to write {}", path.display()))
    }

    /// Insert or update the lock entry for `dep`.
    ///
    /// Branches on dep type: git deps write `git`/`rev`/`sha`; archive deps
    /// write `url`/`archive_sha`.
    pub fn upsert(&mut self, dep: &ResolvedDependency) {
        let entry = match dep.dep.kind() {
            DepKind::Git { git, rev } => LockEntry {
                name:          dep.dep.name.clone(),
                git:           Some(git.to_owned()),
                rev:           Some(rev.to_owned()),
                sha:           Some(dep.sha.clone()),
                url:           None,
                archive_sha:   None,
                asset_id:      None,
                asset_version: None,
            },
            DepKind::Archive { url, .. } => LockEntry {
                name:          dep.dep.name.clone(),
                git:           None,
                rev:           None,
                sha:           None,
                url:           Some(url.to_owned()),
                archive_sha:   Some(dep.sha.clone()),
                asset_id:      None,
                asset_version: None,
            },
            DepKind::AssetLib { asset_id } => LockEntry {
                name:          dep.dep.name.clone(),
                git:           None,
                rev:           None,
                sha:           None,
                url:           dep.resolved_url.clone(),
                archive_sha:   Some(dep.sha.clone()),
                asset_id:      Some(asset_id),
                asset_version: dep.asset_version,
            },
        };
        match self.entries.iter_mut().find(|e| e.name == dep.dep.name) {
            Some(existing) => *existing = entry,
            None           => self.entries.push(entry),
        }
    }

    /// Look up a locked commit SHA for a git dependency.
    ///
    /// Returns the SHA only when `name`, `git`, and `rev` all match. Any
    /// change to the dependency in `ggg.toml` invalidates the entry and forces
    /// a fresh resolution.
    pub fn locked_sha(&self, name: &str, git: &str, rev: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| {
                e.name == name
                    && e.git.as_deref() == Some(git)
                    && e.rev.as_deref() == Some(rev)
            })
            .and_then(|e| e.sha.as_deref())
    }

    /// Look up a locked archive SHA for an archive dependency.
    ///
    /// Returns the SHA only when `name` and `url` both match. A URL change in
    /// `ggg.toml` invalidates the entry and forces a fresh download.
    pub fn locked_archive_sha(&self, name: &str, url: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.name == name && e.url.as_deref() == Some(url))
            .and_then(|e| e.archive_sha.as_deref())
    }

    /// Look up the lock entry for a Godot Asset Library dependency.
    ///
    /// Returns the entry only when `name` and `asset_id` both match.
    /// A change to either field invalidates the entry.
    pub fn locked_asset_lib(&self, name: &str, asset_id: u32) -> Option<&LockEntry> {
        self.entries
            .iter()
            .find(|e| e.name == name && e.asset_id == Some(asset_id))
    }

    /// Remove the entry for the dependency with the given `name`, if present.
    pub fn remove(&mut self, name: &str) {
        self.entries.retain(|e| e.name != name);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Dependency;
    use crate::dependency::ResolvedDependency;
    use tempfile::TempDir;

    fn make_resolved(name: &str, sha: &str) -> ResolvedDependency {
        make_resolved_rev(name, "main", sha)
    }

    fn make_resolved_rev(name: &str, rev: &str, sha: &str) -> ResolvedDependency {
        ResolvedDependency {
            dep: Dependency::new_git(name, "https://example.com/repo.git", rev),
            sha: sha.to_string(),
            resolved_url: None,
            asset_version: None,
        }
    }

    fn make_resolved_archive(name: &str, url: &str, archive_sha: &str) -> ResolvedDependency {
        ResolvedDependency {
            dep: Dependency::new_archive(name, url),
            sha: archive_sha.to_string(),
            resolved_url: None,
            asset_version: None,
        }
    }

    #[test]
    fn load_or_empty_returns_empty_when_file_absent() {
        let dir  = TempDir::new().unwrap();
        let lock = LockFile::load_or_empty(&dir.path().join("ggg.lock")).unwrap();
        assert!(lock.entries.is_empty());
    }

    #[test]
    fn upsert_adds_new_entry() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved("gut", &"a".repeat(40)));
        assert_eq!(lock.entries.len(), 1);
        assert_eq!(lock.entries[0].name, "gut");
        assert_eq!(lock.entries[0].sha.as_deref(), Some("a".repeat(40).as_str()));
    }

    #[test]
    fn upsert_updates_existing_entry_without_duplication() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved("gut", &"a".repeat(40)));
        lock.upsert(&make_resolved("gut", &"b".repeat(40)));
        assert_eq!(lock.entries.len(), 1);
        assert_eq!(lock.entries[0].sha.as_deref(), Some("b".repeat(40).as_str()));
    }

    #[test]
    fn upsert_archive_dep() {
        let mut lock = LockFile::default();
        let archive_sha = "e3b0c44298fc1c149afbf4c8996fb924".repeat(2);
        lock.upsert(&make_resolved_archive(
            "debug_draw_3d",
            "https://example.com/debug_draw_3d.zip",
            &archive_sha,
        ));
        assert_eq!(lock.entries.len(), 1);
        assert_eq!(lock.entries[0].url.as_deref(), Some("https://example.com/debug_draw_3d.zip"));
        assert_eq!(lock.entries[0].archive_sha.as_deref(), Some(archive_sha.as_str()));
        assert!(lock.entries[0].git.is_none());
        assert!(lock.entries[0].sha.is_none());
    }

    #[test]
    fn remove_deletes_named_entry_only() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved("gut",     &"a".repeat(40)));
        lock.upsert(&make_resolved("phantom", &"b".repeat(40)));
        lock.remove("gut");
        assert_eq!(lock.entries.len(), 1);
        assert_eq!(lock.entries[0].name, "phantom");
    }

    #[test]
    fn locked_sha_returns_sha_when_all_fields_match() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("gut", "v9.3.0", &"a".repeat(40)));
        assert_eq!(
            lock.locked_sha("gut", "https://example.com/repo.git", "v9.3.0"),
            Some("a".repeat(40).as_str()),
        );
    }

    #[test]
    fn locked_sha_returns_none_when_rev_changed() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("gut", "v9.3.0", &"a".repeat(40)));
        assert!(lock.locked_sha("gut", "https://example.com/repo.git", "v9.4.0").is_none());
    }

    #[test]
    fn locked_sha_returns_none_when_git_changed() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("gut", "main", &"a".repeat(40)));
        assert!(lock.locked_sha("gut", "https://other.example.com/repo.git", "main").is_none());
    }

    #[test]
    fn locked_sha_returns_none_for_unknown_name() {
        let lock = LockFile::default();
        assert!(lock.locked_sha("gut", "https://example.com/repo.git", "main").is_none());
    }

    #[test]
    fn save_load_round_trip() {
        let dir  = TempDir::new().unwrap();
        let path = dir.path().join("ggg.lock");

        let mut lock = LockFile::default();
        lock.upsert(&make_resolved("gut",     &"a".repeat(40)));
        lock.upsert(&make_resolved("phantom", &"b".repeat(40)));
        lock.save(&path).unwrap();

        let loaded = LockFile::load_or_empty(&path).unwrap();
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].name, "gut");
        assert_eq!(loaded.entries[1].name, "phantom");
        assert_eq!(loaded.entries[1].sha.as_deref(), Some("b".repeat(40).as_str()));
    }

    #[test]
    fn locked_archive_sha_returns_sha_when_match() {
        let mut lock = LockFile::default();
        let archive_sha = "e3b0c44298fc1c149afbf4c8996fb924".repeat(2);
        lock.upsert(&make_resolved_archive(
            "foo",
            "https://example.com/foo.zip",
            &archive_sha,
        ));
        assert_eq!(
            lock.locked_archive_sha("foo", "https://example.com/foo.zip"),
            Some(archive_sha.as_str()),
        );
    }

    #[test]
    fn locked_archive_sha_returns_none_when_url_changed() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_archive("foo", "https://example.com/foo_v1.zip", &"a".repeat(64)));
        assert!(lock.locked_archive_sha("foo", "https://example.com/foo_v2.zip").is_none());
    }
}
