pub mod cache;
pub mod download;
pub mod install;
pub mod lockfile;
pub mod resolver;
pub mod state;

use crate::config::Dependency;

/// A [`Dependency`] with its `rev` resolved to a canonical 40-character commit SHA.
///
/// Produced by [`resolver::resolve`]. All subsequent pipeline stages (fetch,
/// install, lock file) operate on this type rather than the raw config entry.
pub struct ResolvedDependency {
    /// The original config entry, including the unresolved `rev` label.
    pub dep: Dependency,
    /// Resolved 40-character lowercase hex commit SHA.
    pub sha: String,
}

