//! Committed lock file (`ggg.lock`) recording the resolved commit SHA for each
//! dependency.
//!
//! This is the reproducibility record: it is committed to git alongside the
//! project so that any checkout can re-install the exact same dependency
//! versions without network resolution.
//!
//! Local install tracking (which files were written into this working tree) is
//! stored separately in `.ggg.state`, which is gitignored.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{Config, Dependency, Source, SourceKind};
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

/// One discrepancy found between `ggg.lock` and `ggg.toml` by
/// [`LockFile::check_all_dependencies`].
///
/// `message` is a human-readable explanation that points at `ggg sync` as the
/// reconciliation step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    /// The dependency name involved.
    pub name: String,
    /// What is wrong and how to fix it.
    pub message: String,
}

/// One dependency record inside `ggg.lock`.
///
/// Git entries have `git`, `rev`, and `sha` set.
/// Archive entries have `url` and `archive_sha` set.
/// Asset library entries have `asset_library_id`, `asset_version`, `url`, and
/// `archive_sha` set.
/// Asset store entries have `publisher_slug`, `asset_slug`, `release_id`,
/// `release_version`, `url`, and `archive_sha` set.
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
    /// asset library / asset store API (asset deps).  Together with `name`
    /// forms the lock key for archive deps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// SHA-256 hex digest of the downloaded archive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_sha: Option<String>,

    // --- asset library dep fields ---
    /// Numeric asset ID from the Godot Asset Library. Together with `name`
    /// forms the lock key for asset lib deps.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "asset_id")]
    pub asset_library_id: Option<u32>,
    /// Asset library version integer at the time the lock was written.
    /// Used by `ggg update` to detect whether a newer version is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_version: Option<u32>,

    // --- asset store dep fields ---
    /// The store publisher slug from `ggg.toml` (`publisher_slug/asset_slug:version`).
    /// Together with `name`, `asset_slug`, and `release_version` forms the
    /// lock key for asset store deps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher_slug: Option<String>,
    /// The store asset slug from `ggg.toml`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_slug: Option<String>,
    /// Numeric release ID of the resolved Asset Store release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<u64>,
    /// Human-readable version string of the resolved Asset Store release,
    /// e.g. `"1.2.3"`. Used by `ggg update` to detect whether a newer
    /// version is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_version: Option<String>,
}

impl LockEntry {
    /// The [`SourceKind`] recorded by this lock entry, or `None` when the
    /// entry is empty or ambiguous (e.g. both asset library and store fields
    /// set - such an entry can never be produced by [`LockFile::upsert`]).
    pub fn kind(&self) -> Option<SourceKind> {
        let is_asset_lib = self.asset_library_id.is_some();
        let is_store = self.publisher_slug.is_some()
            || self.asset_slug.is_some()
            || self.release_id.is_some()
            || self.release_version.is_some();
        match (is_asset_lib, is_store) {
            (true, false) => Some(SourceKind::AssetLib),
            (false, true) => Some(SourceKind::AssetStore),
            (false, false) if self.git.is_some() => Some(SourceKind::Git),
            (false, false) if self.url.is_some() => Some(SourceKind::Archive),
            _ => None,
        }
    }
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

    /// Serialize and write `ggg.lock` to `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content =
            toml_edit::ser::to_string_pretty(self).context("failed to serialise lock file")?;
        std::fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
    }

    /// Insert or update the lock entry for `dep`.
    ///
    /// Branches on dep type: git deps write `git`/`rev`/`sha`; archive deps
    /// write `url`/`archive_sha`.
    pub fn upsert(&mut self, dep: &ResolvedDependency) {
        let entry = match dep {
            ResolvedDependency::Git(g) => LockEntry {
                name: g.meta.name.clone(),
                git: Some(g.git.clone()),
                rev: Some(g.rev.clone()),
                sha: Some(g.sha.clone()),
                url: None,
                archive_sha: None,
                asset_library_id: None,
                asset_version: None,
                publisher_slug: None,
                asset_slug: None,
                release_id: None,
                release_version: None,
            },
            ResolvedDependency::Archive(a) => LockEntry {
                name: a.meta.name.clone(),
                git: None,
                rev: None,
                sha: None,
                url: Some(a.url.clone()),
                archive_sha: Some(a.sha.clone()),
                asset_library_id: None,
                asset_version: None,
                publisher_slug: None,
                asset_slug: None,
                release_id: None,
                release_version: None,
            },
            ResolvedDependency::AssetLib(a) => LockEntry {
                name: a.meta.name.clone(),
                git: None,
                rev: None,
                sha: None,
                url: Some(a.resolved_url.clone()),
                archive_sha: Some(a.sha.clone()),
                asset_library_id: Some(a.asset_library_id),
                asset_version: a.asset_version,
                publisher_slug: None,
                asset_slug: None,
                release_id: None,
                release_version: None,
            },
            ResolvedDependency::AssetStore(a) => LockEntry {
                name: a.meta.name.clone(),
                git: None,
                rev: None,
                sha: None,
                url: Some(a.resolved_url.clone()),
                archive_sha: Some(a.sha.clone()),
                asset_library_id: None,
                asset_version: None,
                publisher_slug: Some(a.publisher.clone()),
                asset_slug: Some(a.asset.clone()),
                release_id: a.release_id,
                release_version: a.release_version.clone(),
            },
        };
        match self.entries.iter_mut().find(|e| e.name == dep.name()) {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
    }

    /// The lock entry for `dep` when it is both faithful to `ggg.toml` (no
    /// drift) and complete enough to reuse (carries every resolved-value
    /// field). Returns `None` when the entry is missing, drifted, or
    /// incomplete - in all cases the caller should resolve fresh.
    pub(crate) fn verified_entry(&self, dep: &Dependency) -> Option<&LockEntry> {
        self.get_entry(&dep.name)
            .filter(|e| determine_drift_status(dep, Some(*e)).is_none() && is_complete(dep, e))
    }

    /// Remove the entry for the dependency with the given `name`, if present.
    pub fn remove(&mut self, name: &str) {
        self.entries.retain(|e| e.name != name);
    }

    /// Report every discrepancy between this lock file and `config`.
    ///
    /// A dependency present in both is checked for its source kind and for the
    /// lock-key identity recorded at sync time (`git`/`rev`, archive `url`,
    /// `asset_library_id`, or the store publisher/asset/version triple). Any
    /// of the following is reported as drift, which `ggg sync` reconciles:
    /// - a dependency in `config` with no lock entry,
    /// - a lock entry whose name is not in `config`,
    /// - a source-kind mismatch (e.g. locked as `git`, declared `archive`),
    /// - lock-key fields edited in `ggg.toml` since the last sync,
    /// - an asset library lock entry with no recorded `asset_version`,
    /// - a lock entry whose kind cannot be determined (empty or mixed).
    ///
    /// A dependency without a lock entry is drift regardless of its kind:
    /// `ggg update` requires `ggg.lock` to be a faithful snapshot of
    /// `ggg.toml` before it compares versions, so a dep that has never been
    /// synced must be locked first.
    pub fn check_all_dependencies(&self, config: &Config) -> Vec<Drift> {
        let mut drifts = Vec::new();

        for dep in &config.dependency {
            if let Some(drift) = determine_drift_status(dep, self.get_entry(&dep.name)) {
                drifts.push(drift);
            }
        }

        let config_names: HashSet<&str> =
            config.dependency.iter().map(|d| d.name.as_str()).collect();
        for entry in &self.entries {
            if !config_names.contains(entry.name.as_str()) {
                let kind = entry
                    .kind()
                    .map(|k| k.to_string())
                    .unwrap_or_else(|| "unrecognized".to_string());
                drifts.push(Drift {
                    name: entry.name.clone(),
                    message: format!(
                        "{}: lock entry ({kind}) is not in ggg.toml - run `ggg sync` to reconcile.",
                        entry.name
                    ),
                });
            }
        }

        drifts
    }

    /// The [`Drift`] for a single dependency, if any. Used by the named form
    /// of `ggg update` so only the requested dependency is validated.
    pub(crate) fn check_dependency(&self, dep: &Dependency) -> Option<Drift> {
        determine_drift_status(dep, self.get_entry(&dep.name))
    }

    /// The lock entry for `name`, if present.
    pub(crate) fn get_entry(&self, name: &str) -> Option<&LockEntry> {
        self.entries.iter().find(|e| e.name == name)
    }
}

/// The [`Drift`] between one dependency and its lock entry, if any.
fn determine_drift_status(dep: &Dependency, entry: Option<&LockEntry>) -> Option<Drift> {
    let Some(entry) = entry else {
        // update needs the lock file to be a faithful snapshot of ggg.toml
        // before it compares versions, so every dependency must be locked.
        return Some(Drift {
            name: dep.name.clone(),
            message: format!(
                "{}: no lock entry - run `ggg sync` to install and lock the current version.",
                dep.name
            ),
        });
    };

    let config_kind = dep.kind();
    match entry.kind() {
        Some(kind) if kind == config_kind => {
            if !identity_matches(dep, entry) {
                return Some(Drift {
                    name: dep.name.clone(),
                    message: identity_drift_message(dep, entry),
                });
            }
            // Nothing to compare against when the library API recorded no
            // version at sync time.
            if matches!(config_kind, SourceKind::AssetLib) && entry.asset_version.is_none() {
                return Some(Drift {
                    name: dep.name.clone(),
                    message: format!(
                        "{}: no version recorded in ggg.lock - run `ggg sync` to re-lock it.",
                        dep.name
                    ),
                });
            }
            None
        }
        other => Some(Drift {
            name: dep.name.clone(),
            message: match other {
                Some(lock_kind) => format!(
                    "{}: locked as a {lock_kind} dependency but ggg.toml declares it as a {config_kind} dependency - run `ggg sync` to reconcile.",
                    dep.name
                ),
                None => format!(
                    "{}: ggg.lock entry has an unrecognized format - run `ggg sync` to reconcile.",
                    dep.name
                ),
            },
        }),
    }
}

/// Whether the lock entry's identity still matches the dependency's source.
fn identity_matches(dep: &Dependency, entry: &LockEntry) -> bool {
    match &dep.source {
        Source::Git { git, rev } => {
            entry.git.as_deref() == Some(git.as_str()) && entry.rev.as_deref() == Some(rev.as_str())
        }
        Source::Archive { url, .. } => entry.url.as_deref() == Some(url.as_str()),
        Source::AssetLib {
            asset_library_id, ..
        } => entry.asset_library_id == Some(*asset_library_id),
        Source::AssetStore {
            asset_store_asset, ..
        } => {
            entry.publisher_slug.as_deref() == Some(asset_store_asset.publisher.as_str())
                && entry.asset_slug.as_deref() == Some(asset_store_asset.asset.as_str())
                && entry.release_version.as_deref() == Some(asset_store_asset.version.as_str())
        }
    }
}

/// Whether `entry` carries every field needed to rebuild the resolved
/// dependency. `identity_matches` already guarantees the *key* fields
/// (`git`/`rev`, `url`, ids, slugs); this checks the resolved *values*.
fn is_complete(dep: &Dependency, entry: &LockEntry) -> bool {
    match &dep.source {
        Source::Git { .. } => entry.sha.is_some(),
        Source::Archive { .. } => entry.archive_sha.is_some(),
        Source::AssetLib { .. } | Source::AssetStore { .. } => {
            entry.url.is_some() && entry.archive_sha.is_some()
        }
    }
}

/// Explain an identity mismatch in terms of the source kind involved.
fn identity_drift_message(dep: &Dependency, entry: &LockEntry) -> String {
    match &dep.source {
        Source::Git { .. } | Source::Archive { .. } => format!(
            "{}: ggg.lock and ggg.toml disagree on its source (a lock key field was edited in ggg.toml?) - run `ggg sync` to reconcile.",
            dep.name
        ),
        Source::AssetLib {
            asset_library_id, ..
        } => format!(
            "{}: lock entry references asset #{locked} but ggg.toml pins asset #{configured} - run `ggg sync` to reconcile.",
            dep.name,
            locked = entry
                .asset_library_id
                .map_or_else(|| "?".into(), |id| id.to_string()),
            configured = asset_library_id,
        ),
        Source::AssetStore {
            asset_store_asset, ..
        } => format!(
            "{}: locked release {locked} does not match the version {configured} pinned in ggg.toml - run `ggg sync` to reconcile.",
            dep.name,
            locked = entry.release_version.as_deref().unwrap_or("?"),
            configured = asset_store_asset.version,
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_resolved(name: &str, sha: &str) -> ResolvedDependency {
        make_resolved_rev(name, "main", sha)
    }

    fn make_resolved_rev(name: &str, rev: &str, sha: &str) -> ResolvedDependency {
        ResolvedDependency::Git(crate::dependency::GitResolvedDependency {
            meta: crate::dependency::ResolvedMeta {
                name: name.to_owned(),
                map: None,
                exclude: None,
            },
            git: "https://example.com/repo.git".to_owned(),
            rev: rev.to_owned(),
            sha: sha.to_string(),
        })
    }

    fn make_resolved_archive(name: &str, url: &str, archive_sha: &str) -> ResolvedDependency {
        ResolvedDependency::Archive(crate::dependency::ArchiveResolvedDependency {
            meta: crate::dependency::ResolvedMeta {
                name: name.to_owned(),
                map: None,
                exclude: None,
            },
            url: url.to_owned(),
            sha256: None,
            strip_components: None,
            sha: archive_sha.to_string(),
        })
    }

    fn make_resolved_store(
        name: &str,
        publisher: &str,
        asset: &str,
        version: &str,
        sha: &str,
        release_id: u64,
    ) -> ResolvedDependency {
        ResolvedDependency::AssetStore(crate::dependency::AssetStoreResolvedDependency {
            meta: crate::dependency::ResolvedMeta {
                name: name.to_owned(),
                map: None,
                exclude: None,
            },
            publisher: publisher.to_owned(),
            asset: asset.to_owned(),
            version: version.to_owned(),
            strip_components: None,
            resolved_url: format!("https://example.com/{publisher}-{asset}-{version}.zip"),
            sha: sha.to_string(),
            release_id: Some(release_id),
            release_version: Some(version.to_string()),
        })
    }

    #[test]
    fn load_or_empty_returns_empty_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let lock = LockFile::load_or_empty(&dir.path().join("ggg.lock")).unwrap();
        assert!(lock.entries.is_empty());
    }

    #[test]
    fn upsert_adds_new_entry() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved("gut", &"a".repeat(40)));
        assert_eq!(lock.entries.len(), 1);
        assert_eq!(lock.entries[0].name, "gut");
        assert_eq!(
            lock.entries[0].sha.as_deref(),
            Some("a".repeat(40).as_str())
        );
    }

    #[test]
    fn upsert_updates_existing_entry_without_duplication() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved("gut", &"a".repeat(40)));
        lock.upsert(&make_resolved("gut", &"b".repeat(40)));
        assert_eq!(lock.entries.len(), 1);
        assert_eq!(
            lock.entries[0].sha.as_deref(),
            Some("b".repeat(40).as_str())
        );
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
        assert_eq!(
            lock.entries[0].url.as_deref(),
            Some("https://example.com/debug_draw_3d.zip")
        );
        assert_eq!(
            lock.entries[0].archive_sha.as_deref(),
            Some(archive_sha.as_str())
        );
        assert!(lock.entries[0].git.is_none());
        assert!(lock.entries[0].sha.is_none());
    }

    #[test]
    fn remove_deletes_named_entry_only() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved("gut", &"a".repeat(40)));
        lock.upsert(&make_resolved("phantom", &"b".repeat(40)));
        lock.remove("gut");
        assert_eq!(lock.entries.len(), 1);
        assert_eq!(lock.entries[0].name, "phantom");
    }

    #[test]
    fn save_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ggg.lock");

        let mut lock = LockFile::default();
        lock.upsert(&make_resolved("gut", &"a".repeat(40)));
        lock.upsert(&make_resolved("phantom", &"b".repeat(40)));
        lock.save(&path).unwrap();

        let loaded = LockFile::load_or_empty(&path).unwrap();
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].name, "gut");
        assert_eq!(loaded.entries[1].name, "phantom");
        assert_eq!(
            loaded.entries[1].sha.as_deref(),
            Some("b".repeat(40).as_str())
        );
    }

    // --- asset store --------------------------------------------------------

    #[test]
    fn upsert_store_dep_writes_store_fields() {
        let mut lock = LockFile::default();
        let sha = "e3b0c44298fc1c149afbf4c8996fb924".repeat(2);
        lock.upsert(&make_resolved_store(
            "my-addon", "souleat", "my-addon", "1.2.3", &sha, 42,
        ));

        let e = &lock.entries[0];
        assert_eq!(e.name, "my-addon");
        assert_eq!(e.publisher_slug.as_deref(), Some("souleat"));
        assert_eq!(e.asset_slug.as_deref(), Some("my-addon"));
        assert_eq!(e.release_id, Some(42));
        assert_eq!(e.release_version.as_deref(), Some("1.2.3"));
        assert_eq!(
            e.url.as_deref(),
            Some("https://example.com/souleat-my-addon-1.2.3.zip")
        );
        assert_eq!(e.archive_sha.as_deref(), Some(sha.as_str()));
        assert!(e.asset_library_id.is_none());
        assert!(e.asset_version.is_none());
        assert!(e.git.is_none());
    }

    #[test]
    fn store_lock_round_trips_through_save_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ggg.lock");

        let mut lock = LockFile::default();
        let sha = "e3b0c44298fc1c149afbf4c8996fb924".repeat(2);
        lock.upsert(&make_resolved_store(
            "my-addon", "souleat", "my-addon", "1.2.3", &sha, 42,
        ));
        lock.save(&path).unwrap();

        let loaded = LockFile::load_or_empty(&path).unwrap();
        let entry = &loaded.entries[0];
        assert_eq!(entry.name, "my-addon");
        assert_eq!(entry.release_id, Some(42));
        assert_eq!(entry.archive_sha.as_deref(), Some(sha.as_str()));
    }

    #[test]
    fn asset_lib_lock_entry_has_no_store_fields() {
        use crate::dependency::AssetLibResolvedDependency;
        use crate::dependency::ResolvedMeta;

        // Cross-contamination guard: asset-lib and store entries are separate,
        // guaranteed structurally now that ResolvedDependency carries the source
        // kind - a store variant can never be passed to an asset-lib upsert.
        let mut lock = LockFile::default();
        lock.upsert(&ResolvedDependency::AssetLib(AssetLibResolvedDependency {
            meta: ResolvedMeta {
                name: "dialogic".to_owned(),
                map: None,
                exclude: None,
            },
            asset_library_id: 1216,
            strip_components: None,
            resolved_url: "https://example.com/dialogic.zip".to_owned(),
            sha: "e3b0c44298fc1c149afbf4c8996fb924".repeat(2),
            asset_version: Some(3),
        }));

        let e = &lock.entries[0];
        assert_eq!(e.asset_library_id, Some(1216));
        assert_eq!(e.asset_version, Some(3));
        assert!(e.publisher_slug.is_none());
        assert!(e.asset_slug.is_none());
        assert!(e.release_id.is_none());
        assert!(e.release_version.is_none());
    }

    // --- kind classification / config drift ----------------------------------

    // --- verified_entry: faithful and complete lock lookups -----------------

    fn git_dep(name: &str, git: &str, rev: &str) -> Dependency {
        Dependency::new(
            name,
            Source::Git {
                git: git.into(),
                rev: rev.into(),
            },
            None,
            None,
        )
    }

    fn archive_dep(name: &str, url: &str) -> Dependency {
        Dependency::new(
            name,
            Source::Archive {
                url: url.into(),
                sha256: None,
                strip_components: None,
            },
            None,
            None,
        )
    }

    fn asset_lib_dep(name: &str, asset_library_id: u32) -> Dependency {
        Dependency::new(
            name,
            Source::AssetLib {
                asset_library_id,
                strip_components: None,
            },
            None,
            None,
        )
    }

    #[test]
    fn verified_entry_returns_entry_when_git_lock_is_faithful() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("gut", "main", &"a".repeat(40)));
        let dep = git_dep("gut", "https://example.com/repo.git", "main");
        assert_eq!(
            lock.verified_entry(&dep).unwrap().sha.as_deref(),
            Some("a".repeat(40).as_str())
        );
    }

    #[test]
    fn verified_entry_returns_entry_when_archive_lock_is_faithful() {
        let mut lock = LockFile::default();
        let sha = "b".repeat(64);
        lock.upsert(&make_resolved_archive(
            "foo",
            "https://example.com/foo.zip",
            &sha,
        ));
        let dep = archive_dep("foo", "https://example.com/foo.zip");
        assert_eq!(
            lock.verified_entry(&dep).unwrap().archive_sha.as_deref(),
            Some(sha.as_str())
        );
    }

    #[test]
    fn verified_entry_returns_entry_when_asset_lib_lock_is_faithful() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_asset_lib("d", 5, Some(1)));
        let dep = asset_lib_dep("d", 5);
        assert_eq!(lock.verified_entry(&dep).unwrap().asset_library_id, Some(5));
    }

    #[test]
    fn verified_entry_returns_entry_when_store_lock_is_faithful() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_store(
            "my-addon",
            "souleat",
            "my-addon",
            "1.2.3",
            &"c".repeat(64),
            42,
        ));
        let dep = Dependency::new(
            "my-addon",
            store_source("souleat/my-addon:1.2.3"),
            None,
            None,
        );
        assert_eq!(lock.verified_entry(&dep).unwrap().release_id, Some(42));
    }

    #[test]
    fn verified_entry_returns_none_when_no_entry() {
        let lock = LockFile::default();
        let dep = git_dep("gut", "https://example.com/repo.git", "main");
        assert!(lock.verified_entry(&dep).is_none());
    }

    #[test]
    fn verified_entry_returns_none_on_kind_mismatch() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("foo", "main", &"a".repeat(40)));
        let dep = archive_dep("foo", "https://example.com/foo.zip");
        assert!(lock.verified_entry(&dep).is_none());
    }

    #[test]
    fn verified_entry_returns_none_when_git_rev_or_url_edited() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("gut", "v1.0.0", &"a".repeat(40)));
        let dep = git_dep("gut", "https://example.com/repo.git", "v2.0.0");
        assert!(lock.verified_entry(&dep).is_none());
    }

    #[test]
    fn verified_entry_returns_none_when_archive_url_edited() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_archive(
            "foo",
            "https://example.com/foo_v1.zip",
            &"a".repeat(64),
        ));
        let dep = archive_dep("foo", "https://example.com/foo_v2.zip");
        assert!(lock.verified_entry(&dep).is_none());
    }

    #[test]
    fn verified_entry_returns_none_when_asset_lib_id_edited() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_asset_lib("d", 5, Some(1)));
        let dep = asset_lib_dep("d", 7);
        assert!(lock.verified_entry(&dep).is_none());
    }

    #[test]
    fn verified_entry_returns_none_when_store_version_edited() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_store(
            "my-addon",
            "souleat",
            "my-addon",
            "1.2.3",
            &"c".repeat(64),
            42,
        ));
        let dep = Dependency::new(
            "my-addon",
            store_source("souleat/my-addon:2.0.0"),
            None,
            None,
        );
        assert!(lock.verified_entry(&dep).is_none());
    }

    #[test]
    fn verified_entry_returns_none_when_git_entry_missing_sha() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("gut", "main", &"a".repeat(40)));
        lock.entries[0].sha = None;
        let dep = git_dep("gut", "https://example.com/repo.git", "main");
        assert!(lock.verified_entry(&dep).is_none());
    }

    #[test]
    fn verified_entry_returns_none_when_archive_entry_missing_sha() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_archive(
            "foo",
            "https://example.com/foo.zip",
            &"a".repeat(64),
        ));
        lock.entries[0].archive_sha = None;
        let dep = archive_dep("foo", "https://example.com/foo.zip");
        assert!(lock.verified_entry(&dep).is_none());
    }

    fn config_with(deps: Vec<(&str, Source)>) -> Config {
        Config {
            project: crate::config::Project {
                godot: "4.3-stable".parse().unwrap(),
                export_templates: false,
            },
            sync: None,
            dependency: deps
                .into_iter()
                .map(|(name, source)| Dependency::new(name, source, None, None))
                .collect(),
        }
    }

    fn store_source(spec: &str) -> Source {
        Source::AssetStore {
            asset_store_asset: spec.parse().unwrap(),
            strip_components: None,
        }
    }

    fn make_resolved_asset_lib(
        name: &str,
        asset_library_id: u32,
        asset_version: Option<u32>,
    ) -> ResolvedDependency {
        use crate::dependency::AssetLibResolvedDependency;
        use crate::dependency::ResolvedMeta;
        ResolvedDependency::AssetLib(AssetLibResolvedDependency {
            meta: ResolvedMeta {
                name: name.to_owned(),
                map: None,
                exclude: None,
            },
            asset_library_id,
            strip_components: None,
            resolved_url: format!("https://example.com/{name}.zip"),
            sha: "a".repeat(64),
            asset_version,
        })
    }

    #[test]
    fn lock_entry_kind_classifies_by_fields() {
        let empty = LockEntry {
            name: "e".into(),
            git: None,
            rev: None,
            sha: None,
            url: None,
            archive_sha: None,
            asset_library_id: None,
            asset_version: None,
            publisher_slug: None,
            asset_slug: None,
            release_id: None,
            release_version: None,
        };
        assert_eq!(empty.kind(), None);

        let mut repo = LockFile::default();
        repo.upsert(&make_resolved_rev("g", "main", &"a".repeat(40)));
        assert_eq!(repo.entries[0].kind(), Some(SourceKind::Git));

        let mut ar = LockFile::default();
        ar.upsert(&make_resolved_archive(
            "a",
            "https://example.com/z.zip",
            &"a".repeat(64),
        ));
        assert_eq!(ar.entries[0].kind(), Some(SourceKind::Archive));

        let mut lib = LockFile::default();
        lib.upsert(&make_resolved_asset_lib("d", 5, Some(1)));
        assert_eq!(lib.entries[0].kind(), Some(SourceKind::AssetLib));

        let mut st = LockFile::default();
        st.upsert(&make_resolved_store(
            "s",
            "pub",
            "slug",
            "1.0.0",
            &"a".repeat(64),
            7,
        ));
        assert_eq!(st.entries[0].kind(), Some(SourceKind::AssetStore));
    }

    #[test]
    fn drift_empty_when_lock_matches_config() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_store(
            "my-addon",
            "souleat",
            "my-addon",
            "1.0.0",
            &"a".repeat(64),
            3,
        ));
        let config = config_with(vec![("my-addon", store_source("souleat/my-addon:1.0.0"))]);
        assert!(lock.check_all_dependencies(&config).is_empty());
    }

    #[test]
    fn drift_reports_config_dep_missing_from_lock() {
        let config = config_with(vec![("my-addon", store_source("souleat/my-addon:1.0.0"))]);
        let drifts = LockFile::default().check_all_dependencies(&config);
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].message.contains("no lock entry"));
        assert!(drifts[0].message.contains("ggg sync"));
    }

    #[test]
    fn drift_reports_git_dep_without_lock_entry() {
        let config = config_with(vec![(
            "foo",
            Source::Git {
                git: "https://example.com/foo.git".into(),
                rev: "main".into(),
            },
        )]);
        let drifts = LockFile::default().check_all_dependencies(&config);
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].message.contains("no lock entry"));
        assert!(drifts[0].message.contains("ggg sync"));
    }

    #[test]
    fn drift_reports_archive_dep_without_lock_entry() {
        let config = config_with(vec![(
            "foo",
            Source::Archive {
                url: "https://example.com/foo.tar.gz".into(),
                sha256: None,
                strip_components: None,
            },
        )]);
        let drifts = LockFile::default().check_all_dependencies(&config);
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].message.contains("no lock entry"));
        assert!(drifts[0].message.contains("ggg sync"));
    }

    #[test]
    fn drift_reports_lock_entry_not_in_config() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("orphan", "main", &"a".repeat(40)));
        let drifts = lock.check_all_dependencies(&config_with(vec![]));
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].message.contains("orphan"));
        assert!(drifts[0].message.contains("not in ggg.toml"));
    }

    #[test]
    fn drift_reports_kind_mismatch() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("foo", "main", &"a".repeat(40)));
        let config = config_with(vec![("foo", store_source("souleat/foo:1.0.0"))]);
        let drifts = lock.check_all_dependencies(&config);
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].message.contains("locked as a git dependency"));
        assert!(drifts[0].message.contains("asset-store"));
    }

    #[test]
    fn drift_reports_git_archive_kind_swap() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("foo", "main", &"a".repeat(40)));
        let config = config_with(vec![(
            "foo",
            Source::Archive {
                url: "https://example.com/foo.zip".into(),
                sha256: None,
                strip_components: None,
            },
        )]);
        let drifts = lock.check_all_dependencies(&config);
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].message.contains("locked as a git dependency"));
        assert!(drifts[0].message.contains("archive"));
    }

    #[test]
    fn drift_reports_store_version_edit() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_store(
            "my-addon",
            "souleat",
            "my-addon",
            "1.0.0",
            &"a".repeat(64),
            3,
        ));
        let config = config_with(vec![("my-addon", store_source("souleat/my-addon:2.0.0"))]);
        let drifts = lock.check_all_dependencies(&config);
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].message.contains("locked release 1.0.0"));
        assert!(drifts[0].message.contains("version 2.0.0"));
    }

    #[test]
    fn drift_reports_asset_lib_id_edit() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_asset_lib("d", 5, Some(1)));
        let config = config_with(vec![(
            "d",
            Source::AssetLib {
                asset_library_id: 7,
                strip_components: None,
            },
        )]);
        let drifts = lock.check_all_dependencies(&config);
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].message.contains("asset #5"));
        assert!(drifts[0].message.contains("asset #7"));
    }

    #[test]
    fn drift_reports_asset_lib_without_recorded_version() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_asset_lib("d", 5, None));
        let config = config_with(vec![(
            "d",
            Source::AssetLib {
                asset_library_id: 5,
                strip_components: None,
            },
        )]);
        let drifts = lock.check_all_dependencies(&config);
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].message.contains("no version recorded"));
    }

    #[test]
    fn drift_reports_git_rev_edit() {
        let mut lock = LockFile::default();
        lock.upsert(&make_resolved_rev("foo", "v1.0.0", &"a".repeat(40)));
        let config = config_with(vec![(
            "foo",
            Source::Git {
                git: "https://example.com/foo.git".into(),
                rev: "v2.0.0".into(),
            },
        )]);
        let drifts = lock.check_all_dependencies(&config);
        assert_eq!(drifts.len(), 1);
        assert!(drifts[0].message.contains("disagree on its source"));
    }
}
