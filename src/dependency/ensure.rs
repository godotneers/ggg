//! Ensuring a dependency is resolved, downloaded, and available in the cache.
//!
//! [`ensure_dependency`] is the top-level entry point: given a raw config
//! [`Dependency`] it resolves the version, downloads if necessary, and returns
//! the fully-populated [`ResolvedDependency`].  It mirrors the pattern used by
//! [`crate::godot::engine::ensure`] for Godot engine binaries.
//!
//! [`ensure`] is the lower-level primitive that operates on an already-resolved
//! dependency and handles the cache check / download / install step.  For
//! archive and asset-library dependencies the archive SHA may not be known
//! until the file is downloaded; [`ensure`] computes it and returns an updated
//! [`ResolvedDependency`] with `sha` filled in.

use anyhow::{Context, Result};

use crate::config::{DepKind, Dependency};
use crate::dependency::cache::DependencyCache;
use crate::dependency::download;
use crate::dependency::lockfile::LockFile;
use crate::dependency::resolver;
use crate::dependency::ResolvedDependency;

/// Resolve `dep` and ensure it is present in `cache`.
///
/// Combines resolution (SHA/URL lookup from lock or network) with the
/// download/cache step.  Returns the fully-populated [`ResolvedDependency`]
/// and a short human-readable note describing how the version was obtained.
///
/// For git deps whose locked commit is no longer available, the dep is
/// automatically re-resolved from the configured rev.
pub fn ensure_dependency(
    dep: &Dependency,
    lock: &LockFile,
    cache: &DependencyCache,
) -> Result<(ResolvedDependency, String)> {
    let git_used_lock = if let DepKind::Git { git, rev } = dep.kind() {
        lock.locked_sha(&dep.name, git, rev).is_some()
    } else {
        false
    };

    let (resolved, note) = resolver::resolve_dependency(dep, lock)
        .with_context(|| format!("failed to resolve dependency {:?}", dep.name))?;

    let locked_sha_prefix = resolved.sha[..resolved.sha.len().min(12)].to_owned();

    let resolved = match ensure(resolved, cache) {
        Ok(r) => r,
        Err(e) if git_used_lock => {
            let rev = dep.rev.as_deref().unwrap_or("?");
            eprintln!(
                "  warning: locked commit {} for {:?} is no longer available ({}); \
                 re-resolving from {:?}",
                locked_sha_prefix, dep.name, e, rev
            );
            let re_resolved = resolver::resolve(dep)
                .with_context(|| format!("failed to re-resolve dependency {:?}", dep.name))?;
            let re_sha_prefix = re_resolved.sha[..12].to_owned();
            let re_resolved = ensure(re_resolved, cache)
                .with_context(|| format!("failed to download dependency {:?}", dep.name))?;
            return Ok((re_resolved, format!("re-resolved {re_sha_prefix}")));
        }
        Err(e) => {
            return Err(e)
                .with_context(|| format!("failed to download dependency {:?}", dep.name))
        }
    };

    let note = if note.is_empty() {
        format!("downloaded {}", &resolved.sha[..8])
    } else {
        note
    };

    Ok((resolved, note))
}

/// Ensure `resolved` is present in `cache`, downloading and installing it if
/// not.  Returns the (possibly updated) [`ResolvedDependency`]; for
/// archive/asset-library deps whose SHA was unknown on entry, `resolved.sha`
/// is filled in from the downloaded archive.
fn ensure(resolved: ResolvedDependency, cache: &DependencyCache) -> Result<ResolvedDependency> {
    if !resolved.sha.is_empty() && cache.contains(&resolved) {
        return Ok(resolved);
    }

    println!("  {} - downloading...", resolved.dep.name);

    let (path, sha) = download::download(&resolved)
        .with_context(|| format!("failed to download {:?}", resolved.dep.name))?;
    let resolved = ResolvedDependency { sha, ..resolved };

    if !cache.contains(&resolved) {
        cache.install(&resolved, &path)
            .with_context(|| format!("failed to install {:?} into cache", resolved.dep.name))?;
    }

    download::cleanup(&path);
    Ok(resolved)
}
