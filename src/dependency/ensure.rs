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

use crate::config::{Dependency, Source};
use crate::dependency::ResolvedDependency;
use crate::dependency::cache::DependencyCache;
use crate::dependency::download;
use crate::dependency::lockfile::LockFile;
use crate::dependency::resolver;
use crate::godot::release::GodotVersion;

/// Resolve `dep` and ensure it is present in `cache`.
///
/// Combines resolution (SHA/URL lookup from lock or network) with the
/// download/cache step.  Returns the fully-populated [`ResolvedDependency`]
/// and a short human-readable note describing how the version was obtained.
///
/// `godot_version` is the project's Godot version; it is required for
/// Asset Store deps so the resolver can warn when the pinned release's
/// compatibility range does not cover the project.
///
/// For git deps whose locked commit is no longer available, the dep is
/// automatically re-resolved from the configured rev.
pub fn ensure_dependency(
    dep: &Dependency,
    lock: &LockFile,
    cache: &DependencyCache,
    godot_version: Option<&GodotVersion>,
) -> Result<(ResolvedDependency, String)> {
    let git_used_lock =
        matches!(&dep.source, Source::Git { .. }) && lock.verified_entry(dep).is_some();

    let (resolved, note) = resolver::resolve_dependency(dep, lock, godot_version)
        .with_context(|| format!("failed to resolve dependency {:?}", dep.name))?;

    let locked_sha_prefix = {
        let s = resolved.sha();
        s[..s.len().min(12)].to_owned()
    };

    let resolved = match ensure(resolved, cache) {
        Ok(r) => r,
        Err(e) if git_used_lock => {
            let rev = match &dep.source {
                Source::Git { rev, .. } => rev.as_str(),
                _ => "?",
            };
            eprintln!(
                "  warning: locked commit {} for {:?} is no longer available ({}); \
                 re-resolving from {:?}",
                locked_sha_prefix, dep.name, e, rev
            );
            let re_resolved = resolver::resolve(dep)
                .with_context(|| format!("failed to re-resolve dependency {:?}", dep.name))?;
            let re_sha_prefix = re_resolved.sha()[..12].to_owned();
            let re_resolved = ensure(re_resolved, cache)
                .with_context(|| format!("failed to download dependency {:?}", dep.name))?;
            return Ok((re_resolved, format!("re-resolved {re_sha_prefix}")));
        }
        Err(e) => {
            return Err(e).with_context(|| format!("failed to download dependency {:?}", dep.name));
        }
    };

    let note = if note.is_empty() {
        format!("downloaded {}", &resolved.sha()[..8])
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
    if !resolved.sha().is_empty() && cache.contains(&resolved) {
        return Ok(resolved);
    }

    println!("  {} - downloading...", resolved.name());

    let (path, sha) = download::download(&resolved)
        .with_context(|| format!("failed to download {:?}", resolved.name()))?;
    let resolved = resolved.with_sha(sha);

    if !cache.contains(&resolved) {
        cache
            .install(&resolved, &path)
            .with_context(|| format!("failed to install {:?} into cache", resolved.name()))?;
    }

    download::cleanup(&path);
    Ok(resolved)
}
