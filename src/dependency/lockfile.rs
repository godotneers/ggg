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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LockEntry {
    /// Matches the `name` field in `ggg.toml`.
    pub name: String,
    /// The git URL from `ggg.toml`.
    pub git: String,
    /// The `rev` value from `ggg.toml` (branch, tag, or SHA) at the time this
    /// entry was written. Together with `name` and `git` this forms the lock
    /// key: if any of the three change, the entry no longer applies and the
    /// dependency must be re-resolved to restore reproducibility.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rev: String,
    /// Resolved 40-character lowercase hex commit SHA.
    pub sha: String,
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

    /// Insert or update the entry for `dep`.
    pub fn upsert(&mut self, dep: &ResolvedDependency) {
        let entry = LockEntry {
            name: dep.dep.name.clone(),
            git:  dep.dep.git.clone(),
            rev:  dep.dep.rev.clone(),
            sha:  dep.sha.clone(),
        };
        match self.entries.iter_mut().find(|e| e.name == dep.dep.name) {
            Some(existing) => *existing = entry,
            None           => self.entries.push(entry),
        }
    }

    /// Look up a locked SHA for a dependency.
    ///
    /// Returns the SHA only when `name`, `git`, and `rev` all match. This is
    /// the reproducibility guarantee: as long as the dependency is unchanged in
    /// `ggg.toml`, every sync installs the same commit. Any change to the
    /// dependency invalidates the lock entry and forces a fresh resolution,
    /// after which the new SHA is written back to the lock file.
    pub fn locked_sha(&self, name: &str, git: &str, rev: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.name == name && e.git == git && e.rev == rev)
            .map(|e| e.sha.as_str())
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
            dep: Dependency {
                name: name.to_string(),
                git:  "https://example.com/repo.git".to_string(),
                rev:  rev.to_string(),
                map:  None,
            },
            sha: sha.to_string(),
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
        assert_eq!(lock.entries[0].sha,  "a".repeat(40));
    }

    #[test]
    fn upsert_updates_existing_entry_without_duplication() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved("gut", &"a".repeat(40)));
        lock.upsert(&make_resolved("gut", &"b".repeat(40)));
        assert_eq!(lock.entries.len(), 1);
        assert_eq!(lock.entries[0].sha, "b".repeat(40));
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
        assert_eq!(loaded.entries[1].sha,  "b".repeat(40));
    }
}
