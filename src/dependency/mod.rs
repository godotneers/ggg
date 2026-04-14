pub mod archive;
pub mod cache;
pub mod download;
pub mod install;
pub mod lockfile;
pub mod resolver;
pub mod state;

use crate::config::Dependency;

/// A [`Dependency`] paired with its resolved version identity.
///
/// For git deps: `sha` is the resolved 40-character commit SHA.
/// For archive deps: `sha` is the SHA-256 hex digest of the downloaded archive.
///
/// All pipeline stages after resolution (cache lookup, install, lock file
/// upsert) operate on this type rather than the raw config entry.
pub struct ResolvedDependency {
    /// The original config entry.
    pub dep: Dependency,
    /// Version identity - commit SHA for git deps, archive SHA-256 for
    /// archive deps.
    pub sha: String,
}

