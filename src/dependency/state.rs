//! Gitignored local state file (`.ggg.state`) tracking which files GGG
//! installed into this particular working tree.
//!
//! # Why this exists
//!
//! `ggg.lock` records the resolved commit SHAs and is committed to git.
//! `.ggg.state` records the actual files written to disk in this checkout
//! so that `ggg sync` can distinguish GGG-managed files from user files
//! without depending on the shared cache (which may be on a different machine
//! or have been cleared).
//!
//! # Ownership semantics
//!
//! A file at path `P` is considered GGG-owned if and only if:
//! - `P` appears in this state file, AND
//! - the current on-disk SHA-256 hash of `P` matches the hash recorded at
//!   install time.
//!
//! If the hash differs the user has modified the file since it was installed,
//! and GGG will not overwrite it (unless `--force` is given).
//!
//! # Recovery
//!
//! If `.ggg.state` is absent (e.g. after `git reset --hard`), `ggg sync`
//! enters recovery mode: a file whose on-disk hash matches what GGG would
//! install is treated as GGG-owned and overwritten safely.  After a successful
//! sync a fresh state file is written.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Filename of the local state file, relative to the project root.
pub const STATE_FILE: &str = ".ggg.state";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Contents of `.ggg.state`.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct LocalState {
    #[serde(default, rename = "dependency")]
    pub entries: Vec<StateEntry>,
}

/// Per-dependency record of all files GGG installed.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateEntry {
    /// Matches the `name` field in `ggg.toml`.
    pub name: String,
    /// Files written for this dependency.
    #[serde(default)]
    pub files: Vec<InstalledFile>,
}

/// A single file written by GGG.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InstalledFile {
    /// Project-relative path, always using forward slashes.
    pub path: String,
    /// Lowercase hex SHA-256 of the file content at install time.
    pub hash: String,
}

// ---------------------------------------------------------------------------
// impl LocalState
// ---------------------------------------------------------------------------

impl LocalState {
    /// Load `.ggg.state` from `path`.
    ///
    /// Returns `(state, true)` when the file exists, or `(empty, false)` when
    /// it does not.  The boolean lets the caller know whether to apply recovery
    /// heuristics.
    pub fn load_or_empty(path: &Path) -> Result<(Self, bool)> {
        if !path.exists() {
            return Ok((Self::default(), false));
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let state = toml_edit::de::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok((state, true))
    }

    /// Serialise and write `.ggg.state` to `path`.
    ///
    /// If the file already exists and is read-only (the normal state after a
    /// previous sync), it is made writable first, then written, then made
    /// read-only again.  The read-only bit is a low-cost signal to tools like
    /// git that the file should not be modified, adding friction against
    /// accidental deletion via `git reset --hard` or similar.
    pub fn save(&self, path: &Path) -> Result<()> {
        // If a previous write left the file read-only, unlock it first.
        if path.exists() {
            let mut perms = std::fs::metadata(path)
                .with_context(|| format!("failed to read metadata of {}", path.display()))?
                .permissions();
            if perms.readonly() {
                perms.set_readonly(false);
                std::fs::set_permissions(path, perms)
                    .with_context(|| format!("failed to make {} writable", path.display()))?;
            }
        }

        let content = toml_edit::ser::to_string_pretty(self)
            .context("failed to serialise local state")?;
        std::fs::write(path, &content)
            .with_context(|| format!("failed to write {}", path.display()))?;

        // Lock the file to discourage accidental modification.
        let mut perms = std::fs::metadata(path)
            .with_context(|| format!("failed to read metadata of {}", path.display()))?
            .permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(path, perms)
            .with_context(|| format!("failed to make {} read-only", path.display()))?;

        Ok(())
    }

    /// Returns `true` if `rel_path` with content `hash` is owned by GGG.
    ///
    /// Both the path and the hash must match.  A file whose content has changed
    /// since install is NOT considered GGG-owned.
    pub fn is_owned(&self, rel_path: &str, hash: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.files.iter().any(|f| f.path == rel_path && f.hash == hash))
    }

    /// Returns `true` if `rel_path` appears in any dependency's file list,
    /// regardless of whether the content has changed.
    pub fn is_managed_path(&self, rel_path: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.files.iter().any(|f| f.path == rel_path))
    }

    /// Insert or replace the entry for the named dependency.
    pub fn upsert_entry(&mut self, entry: StateEntry) {
        match self.entries.iter_mut().find(|e| e.name == entry.name) {
            Some(existing) => *existing = entry,
            None           => self.entries.push(entry),
        }
    }

    /// Remove the entry for the named dependency, if present.
    pub fn remove_entry(&mut self, name: &str) {
        self.entries.retain(|e| e.name != name);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_entry(name: &str, path: &str, hash: &str) -> StateEntry {
        StateEntry {
            name:  name.to_string(),
            files: vec![InstalledFile {
                path: path.to_string(),
                hash: hash.to_string(),
            }],
        }
    }

    fn state_with(name: &str, path: &str, hash: &str) -> LocalState {
        let mut s = LocalState::default();
        s.upsert_entry(make_entry(name, path, hash));
        s
    }

    // --- ownership queries --------------------------------------------------

    #[test]
    fn is_owned_true_when_path_and_hash_match() {
        let state = state_with("gut", "addons/gut/gut.gd", "abc123");
        assert!(state.is_owned("addons/gut/gut.gd", "abc123"));
    }

    #[test]
    fn is_owned_false_when_hash_differs() {
        let state = state_with("gut", "addons/gut/gut.gd", "abc123");
        assert!(!state.is_owned("addons/gut/gut.gd", "different"));
    }

    #[test]
    fn is_owned_false_when_path_differs() {
        let state = state_with("gut", "addons/gut/gut.gd", "abc123");
        assert!(!state.is_owned("other/path.gd", "abc123"));
    }

    #[test]
    fn is_managed_path_true_regardless_of_hash() {
        let state = state_with("gut", "addons/gut/gut.gd", "abc123");
        assert!(state.is_managed_path("addons/gut/gut.gd"));
    }

    #[test]
    fn is_managed_path_false_when_not_present() {
        let state = state_with("gut", "addons/gut/gut.gd", "abc123");
        assert!(!state.is_managed_path("other/path.gd"));
    }

    // --- persistence --------------------------------------------------------

    #[test]
    fn load_or_empty_returns_false_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let (_, present) = LocalState::load_or_empty(&dir.path().join(STATE_FILE)).unwrap();
        assert!(!present);
    }

    #[test]
    fn load_or_empty_returns_true_and_data_when_file_exists() {
        let dir  = TempDir::new().unwrap();
        let path = dir.path().join(STATE_FILE);
        state_with("gut", "addons/gut/gut.gd", "abc123").save(&path).unwrap();

        let (loaded, present) = LocalState::load_or_empty(&path).unwrap();
        assert!(present);
        assert!(loaded.is_owned("addons/gut/gut.gd", "abc123"));
    }

    #[test]
    fn save_makes_file_readonly() {
        let dir  = TempDir::new().unwrap();
        let path = dir.path().join(STATE_FILE);
        state_with("gut", "a.gd", "h").save(&path).unwrap();
        assert!(std::fs::metadata(&path).unwrap().permissions().readonly());
    }

    #[test]
    fn save_can_overwrite_readonly_file() {
        let dir  = TempDir::new().unwrap();
        let path = dir.path().join(STATE_FILE);

        state_with("gut", "a.gd", "old").save(&path).unwrap();
        assert!(std::fs::metadata(&path).unwrap().permissions().readonly());

        // Second save over the read-only file must succeed.
        state_with("other", "b.gd", "new").save(&path).unwrap();

        let (loaded, _) = LocalState::load_or_empty(&path).unwrap();
        assert!(loaded.is_owned("b.gd", "new"));
        assert!(!loaded.is_managed_path("a.gd"));
    }

    // --- mutation -----------------------------------------------------------

    #[test]
    fn upsert_adds_new_entry() {
        let mut state = LocalState::default();
        state.upsert_entry(make_entry("gut", "addons/gut.gd", "abc"));
        assert!(state.is_owned("addons/gut.gd", "abc"));
    }

    #[test]
    fn upsert_replaces_existing_entry_without_duplication() {
        let mut state = state_with("gut", "addons/gut.gd", "old");
        state.upsert_entry(StateEntry {
            name:  "gut".to_string(),
            files: vec![InstalledFile { path: "addons/gut.gd".to_string(), hash: "new".to_string() }],
        });
        assert_eq!(state.entries.len(), 1);
        assert!(state.is_owned("addons/gut.gd", "new"));
        assert!(!state.is_owned("addons/gut.gd", "old"));
    }

    #[test]
    fn remove_entry_removes_correct_entry() {
        let mut state = LocalState::default();
        state.upsert_entry(make_entry("gut",   "addons/gut.gd",   "abc"));
        state.upsert_entry(make_entry("other", "other.gd",        "def"));
        state.remove_entry("gut");
        assert!(!state.is_managed_path("addons/gut.gd"));
        assert!(state.is_managed_path("other.gd"));
    }
}
