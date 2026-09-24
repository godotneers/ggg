pub mod cache;
pub mod download;
pub mod ensure;
pub mod lockfile;
pub mod resolver;
pub mod state;
pub mod sync;

use crate::config::MapEntry;

/// One resolved dependency, tagged by its source kind so that every variant
/// only carries the fields that apply to that kind.
///
/// - **Git**: `sha` is the resolved 40-character commit SHA.
/// - **Archive**: `sha` is the SHA-256 hex digest of the downloaded archive.
/// - **AssetLib**: `sha` is the SHA-256 of the downloaded archive, and
///   `resolved_url` holds the download URL from the asset library API or lock.
/// - **AssetStore**: `sha` is the SHA-256 of the downloaded archive,
///   `resolved_url` holds the (presigned) download URL, and `release_id` /
///   `release_version` identify the exact store release that was resolved.
///
/// The variant *is* the source: there is no embedded config [`Source`] that
/// could disagree with the resolved kind, so downstream stages never need
/// runtime assertions about which fields are set.
///
/// [`Source`]: crate::config::Source
///
/// All pipeline stages after resolution (cache lookup, download, install, lock
/// file upsert) operate on this type rather than the raw config entry.
pub enum ResolvedDependency {
    Git(GitResolvedDependency),
    Archive(ArchiveResolvedDependency),
    AssetLib(AssetLibResolvedDependency),
    AssetStore(AssetStoreResolvedDependency),
}

/// Kind-independent bits that install/lock-file/diff stages need.
#[derive(Debug, Clone)]
pub struct ResolvedMeta {
    /// The dependency name from `ggg.toml`.
    pub name: String,
    /// Path mappings controlling which parts of the source are installed.
    pub map: Option<Vec<MapEntry>>,
    /// Glob patterns excluding destination paths from installation.
    pub exclude: Option<Vec<String>>,
}

/// A resolved git dependency.
///
/// `sha` is the 40-character commit SHA that `rev` resolved to (or passed
/// through directly when `rev` was already a full SHA).
#[derive(Debug, Clone)]
pub struct GitResolvedDependency {
    pub meta: ResolvedMeta,
    /// The git URL from `ggg.toml`.
    pub git: String,
    /// The `rev` from `ggg.toml`.
    pub rev: String,
    /// Resolved commit SHA.
    pub sha: String,
}

/// A resolved archive dependency.
///
/// `sha` is the SHA-256 of the downloaded archive; it may be empty until the
/// archive is downloaded (the `sha256` config field carries the integrity
/// hint instead).
#[derive(Debug, Clone)]
pub struct ArchiveResolvedDependency {
    pub meta: ResolvedMeta,
    /// The archive URL from `ggg.toml`.
    pub url: String,
    /// Optional integrity hint from `ggg.toml`.
    pub sha256: Option<String>,
    /// How many leading path components to strip at install time.
    pub strip_components: Option<u32>,
    /// SHA-256 of the downloaded archive, computed locally.
    pub sha: String,
}

/// A resolved Godot Asset Library dependency.
///
/// `sha` is the SHA-256 of the downloaded archive; `resolved_url` is the
/// download URL from the asset library API or lock file.
#[derive(Debug, Clone)]
pub struct AssetLibResolvedDependency {
    pub meta: ResolvedMeta,
    /// Numeric asset ID from `ggg.toml`.
    pub asset_library_id: u32,
    /// How many leading path components to strip at install time.
    pub strip_components: Option<u32>,
    /// Download URL resolved from the asset library API or lock file.
    pub resolved_url: String,
    /// SHA-256 of the downloaded archive, computed locally.
    pub sha: String,
    /// Asset library version integer at the time of resolution.
    pub asset_version: Option<u32>,
}

/// A resolved Godot Asset Store dependency.
///
/// `sha` is the SHA-256 of the downloaded archive; `resolved_url` is the
/// (presigned) download URL; `release_id` / `release_version` identify the
/// exact store release that was resolved.
#[derive(Debug, Clone)]
pub struct AssetStoreResolvedDependency {
    pub meta: ResolvedMeta,
    /// Store publisher slug from `ggg.toml`.
    pub publisher: String,
    /// Store asset slug from `ggg.toml`.
    pub asset: String,
    /// Pinned version string from `ggg.toml`.
    pub version: String,
    /// How many leading path components to strip at install time.
    pub strip_components: Option<u32>,
    /// Download URL resolved from the store releases API or lock file.
    pub resolved_url: String,
    /// SHA-256 of the downloaded archive, computed locally.
    pub sha: String,
    /// Numeric release ID of the resolved release.
    pub release_id: Option<u64>,
    /// Human-readable version string of the resolved release.
    pub release_version: Option<String>,
}

impl ResolvedDependency {
    /// The dependency name from `ggg.toml`.
    pub fn name(&self) -> &str {
        self.meta().name.as_str()
    }

    /// The kind-independent metadata (name, map, exclude).
    pub fn meta(&self) -> &ResolvedMeta {
        match self {
            ResolvedDependency::Git(d) => &d.meta,
            ResolvedDependency::Archive(d) => &d.meta,
            ResolvedDependency::AssetLib(d) => &d.meta,
            ResolvedDependency::AssetStore(d) => &d.meta,
        }
    }

    /// The resolved version identity: a commit SHA for git deps, the SHA-256
    /// of the downloaded archive for archive/asset deps. May be empty for
    /// archive-like deps until the file is downloaded.
    pub fn sha(&self) -> &str {
        match self {
            ResolvedDependency::Git(d) => &d.sha,
            ResolvedDependency::Archive(d) => &d.sha,
            ResolvedDependency::AssetLib(d) => &d.sha,
            ResolvedDependency::AssetStore(d) => &d.sha,
        }
    }

    /// Path mappings controlling which parts of the source are installed.
    pub fn map(&self) -> Option<&Vec<MapEntry>> {
        self.meta().map.as_ref()
    }

    /// Glob patterns excluding destination paths from installation.
    pub fn exclude(&self) -> Option<&Vec<String>> {
        self.meta().exclude.as_ref()
    }

    /// How many leading path components to strip at install time.
    ///
    /// Applies the per-kind default when the config leaves it unset: archive
    /// deps default to 0, asset-library and asset-store deps default to 1.
    pub fn strip_components(&self) -> u32 {
        match self {
            ResolvedDependency::Git(_) => 0,
            ResolvedDependency::Archive(d) => d.strip_components.unwrap_or(0),
            ResolvedDependency::AssetLib(d) => d.strip_components.unwrap_or(1),
            ResolvedDependency::AssetStore(d) => d.strip_components.unwrap_or(1),
        }
    }

    /// The download URL for archive-like deps ("resolved_url"): the archive
    /// URL from config for archive deps, or the API/lock-resolved URL for
    /// asset deps.
    pub fn archive_url(&self) -> &str {
        match self {
            ResolvedDependency::Git(_) => unreachable!("git deps have no archive URL"),
            ResolvedDependency::Archive(d) => &d.url,
            ResolvedDependency::AssetLib(d) => &d.resolved_url,
            ResolvedDependency::AssetStore(d) => &d.resolved_url,
        }
    }

    /// Replace the resolved version identity (SHA).
    ///
    /// Used by `ensure` after downloading an archive, when the SHA is only
    /// known once the file has been fetched.
    pub fn with_sha(&self, sha: String) -> ResolvedDependency {
        match self {
            ResolvedDependency::Git(d) => {
                ResolvedDependency::Git(GitResolvedDependency { sha, ..d.clone() })
            }
            ResolvedDependency::Archive(d) => {
                ResolvedDependency::Archive(ArchiveResolvedDependency { sha, ..d.clone() })
            }
            ResolvedDependency::AssetLib(d) => {
                ResolvedDependency::AssetLib(AssetLibResolvedDependency { sha, ..d.clone() })
            }
            ResolvedDependency::AssetStore(d) => {
                ResolvedDependency::AssetStore(AssetStoreResolvedDependency { sha, ..d.clone() })
            }
        }
    }
}
