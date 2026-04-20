//! Resolving a dependency to the information needed to download the correct
//! version.
//!
//! For each dependency kind, resolution means something different:
//!
//! - **Git** - a branch name or tag is resolved to a full 40-character commit
//!   SHA via the git protocol.  Bare SHAs pass through without a network call.
//! - **Archive** - the URL is already in the config; the sha256 field (if
//!   present) is carried through as an integrity hint.  No network call.
//! - **Asset Library** - an asset ID is resolved to a download URL via the
//!   Godot Asset Library API.  The archive SHA is not known until the file is
//!   downloaded.
//!
//! [`resolve_dependency`] is the unified entry point.  [`resolve`] is kept as
//! a lower-level git-only helper used by the sync re-resolve fallback.

use anyhow::{bail, Context, Result};
use gix::bstr::ByteSlice;
use gix::progress::Discard;
use gix::protocol::handshake::Ref;

use crate::config::{DepKind, Dependency};
use crate::dependency::lockfile::LockFile;
use crate::dependency::ResolvedDependency;

/// Resolve `dep.rev` to a full commit SHA and return a [`ResolvedDependency`].
///
/// If `dep.rev` is already a 40-character hex string it is validated and
/// returned without making a network connection. Otherwise the remote is
/// queried and the matching ref is dereferenced to its commit SHA.
///
/// # No fetch guarantee
///
/// A successful return does **not** guarantee that the commit can be fetched
/// afterwards. For bare SHAs the remote is never contacted, so existence is
/// never confirmed. For branches and tags the SHA reflects the remote state at
/// the moment of the call - the commit could be force-pushed away or the ref
/// deleted before the subsequent fetch. The fetch step must handle these
/// failure cases regardless.
pub fn resolve(dep: &Dependency) -> Result<ResolvedDependency> {
    let git = dep.git.as_deref()
        .expect("resolver::resolve() called on non-git dependency; check dep type first");
    let rev = dep.rev.as_deref()
        .expect("resolver::resolve() called on dep without rev; validate() not called");

    let sha = if looks_like_sha(rev) {
        rev.to_lowercase()
    } else {
        resolve_remote(git, rev)
            .with_context(|| format!("failed to resolve dependency {:?}", dep.name))?
    };
    Ok(ResolvedDependency { dep: dep.clone(), sha, resolved_url: None, asset_version: None })
}

/// Resolve `dep` against `lock`, returning a [`ResolvedDependency`] that
/// contains enough information to download the correct version and a
/// short human-readable note describing how the version was determined.
///
/// - **Git**: SHA from lock, or from the remote if unlocked.
/// - **Archive**: SHA from lock, or from the `sha256` config field (may be
///   empty if neither is present - the sha is computed post-download).
/// - **Asset Library**: URL + SHA from lock, or URL from the asset library
///   API (SHA is empty until the archive is downloaded).
pub fn resolve_dependency(
    dep: &Dependency,
    lock: &LockFile,
) -> Result<(ResolvedDependency, String)> {
    match dep.kind() {
        DepKind::Git { git, rev } => {
            if let Some(sha) = lock.locked_sha(&dep.name, git, rev) {
                let note = format!("locked {}", &sha[..12]);
                return Ok((
                    ResolvedDependency {
                        dep: dep.clone(),
                        sha: sha.to_owned(),
                        resolved_url: None,
                        asset_version: None,
                    },
                    note,
                ));
            }
            let r = resolve(dep)
                .with_context(|| format!("failed to resolve dependency {:?}", dep.name))?;
            let note = format!("resolved {}", &r.sha[..12]);
            Ok((r, note))
        }

        DepKind::Archive { url, sha256, .. } => {
            if let Some(locked_sha) = lock.locked_archive_sha(&dep.name, url) {
                let note = format!("locked {}", &locked_sha[..8]);
                return Ok((
                    ResolvedDependency {
                        dep: dep.clone(),
                        sha: locked_sha.to_owned(),
                        resolved_url: None,
                        asset_version: None,
                    },
                    note,
                ));
            }
            // sha may be empty if no sha256 field; it will be computed on download.
            Ok((
                ResolvedDependency {
                    dep: dep.clone(),
                    sha: sha256.unwrap_or("").to_owned(),
                    resolved_url: None,
                    asset_version: None,
                },
                String::new(),
            ))
        }

        DepKind::AssetLib { asset_id } => {
            if let Some(entry) = lock.locked_asset_lib(&dep.name, asset_id) {
                let url = entry
                    .url
                    .as_deref()
                    .with_context(|| format!("lock entry for {:?} missing url", dep.name))?;
                let archive_sha = entry
                    .archive_sha
                    .as_deref()
                    .with_context(|| {
                        format!("lock entry for {:?} missing archive_sha", dep.name)
                    })?;
                let version_label = entry
                    .asset_version
                    .map(|v| format!(" (version {})", v))
                    .unwrap_or_default();
                return Ok((
                    ResolvedDependency {
                        dep: dep.clone(),
                        sha: archive_sha.to_owned(),
                        resolved_url: Some(url.to_owned()),
                        asset_version: entry.asset_version,
                    },
                    format!("locked{}", version_label),
                ));
            }

            // No lock entry - resolve the download URL via the asset library API.
            // The archive SHA is not known until the file is actually downloaded.
            println!("  {} - fetching from Godot Asset Library...", dep.name);
            let detail = crate::godot::asset_lib::get_asset(asset_id).with_context(|| {
                format!(
                    "failed to fetch asset {:?} (id={}) from the Godot Asset Library",
                    dep.name, asset_id,
                )
            })?;
            Ok((
                ResolvedDependency {
                    dep: dep.clone(),
                    sha: String::new(),
                    resolved_url: Some(detail.download_url),
                    asset_version: Some(detail.version),
                },
                format!("downloaded v{}", detail.version_string),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Returns `true` if `s` looks like a full 40-character hex SHA.
fn looks_like_sha(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Query the remote for its refs and resolve `rev` to a commit SHA.
fn resolve_remote(url: &str, rev: &str) -> Result<String> {
    let refs = list_remote_refs(url)
        .with_context(|| format!("failed to list refs from {url:?}"))?;

    // Try candidates in priority order:
    // 1. Annotated tag (Peeled) - most specific, `object` is the commit
    // 2. Direct ref matching refs/tags/<rev>  (lightweight tag)
    // 3. Direct ref matching refs/heads/<rev> (branch)
    // 4. Any Direct ref whose name exactly matches rev (e.g. "HEAD")
    let tag_ref  = format!("refs/tags/{rev}");
    let head_ref = format!("refs/heads/{rev}");

    // Annotated tag: server sends a Peeled entry; `object` is the commit SHA.
    for r in &refs {
        if let Ref::Peeled { full_ref_name, object, .. } = r {
            if full_ref_name.as_bstr() == tag_ref.as_bytes().as_bstr() {
                return Ok(object.to_hex().to_string());
            }
        }
    }

    // Lightweight tag, branch, or exact name match.
    for candidate in &[tag_ref.as_str(), head_ref.as_str(), rev] {
        for r in &refs {
            if let Some(sha) = direct_sha(r, candidate) {
                return Ok(sha);
            }
        }
    }

    bail!("ref {rev:?} not found in {url}")
}

/// If `r` is a `Direct` ref whose name matches `name`, return its SHA.
fn direct_sha(r: &Ref, name: &str) -> Option<String> {
    match r {
        Ref::Direct { full_ref_name, object } if full_ref_name.as_bstr() == name.as_bytes().as_bstr() => {
            Some(object.to_hex().to_string())
        }
        _ => None,
    }
}

/// List all refs advertised by the remote using the git upload-pack protocol.
///
/// Uses a temporary bare repository to satisfy gix's API. The directory is
/// discarded immediately after the ls-refs call completes.
fn list_remote_refs(url: &str) -> Result<Vec<Ref>> {
    let url_parsed = gix::url::parse(url.as_bytes().into())
        .with_context(|| format!("invalid git URL: {url:?}"))?;

    let tmp = tempfile::tempdir().context("failed to create temporary directory")?;
    let repo = gix::init_bare(tmp.path())
        .context("failed to initialise temporary repository")?;

    // gix requires a Repository context to open a remote connection - there
    // is no repo-less ls-remote API yet (see GitoxideLabs/gitoxide#930).
    // A temporary bare repository satisfies this requirement with negligible
    // overhead; the directory is discarded as soon as this function returns.
    //
    // Add a wildcard refspec so ref_map asks the server for all refs,
    // not just the subset matched by the remote's default refspecs.
    let remote = repo
        .remote_at(url_parsed)
        .context("failed to configure remote")?
        .with_refspecs(
            ["+refs/*:refs/*"],
            gix::remote::Direction::Fetch,
        )
        .context("failed to configure wildcard refspec")?;

    let connection = remote
        .connect(gix::remote::Direction::Fetch)
        .context("failed to connect to remote")?;

    let (ref_map, _handshake) = connection
        .ref_map(
            Discard,
            gix::remote::ref_map::Options {
                prefix_from_spec_as_filter_on_remote: false,
                ..Default::default()
            },
        )
        .context("failed to retrieve remote refs")?;

    Ok(ref_map.remote_refs)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Dependency, MapEntry};

    fn make_dep(git: &str, rev: &str) -> Dependency {
        Dependency::new_git("test", git, rev)
    }

    #[test]
    fn sha_passthrough_lowercase() {
        let sha = "a".repeat(40);
        let resolved = resolve(&make_dep("https://example.com/repo.git", &sha)).unwrap();
        assert_eq!(resolved.sha, sha);
    }

    #[test]
    fn sha_passthrough_uppercase_normalised() {
        let upper = "A".repeat(40);
        let lower = "a".repeat(40);
        let resolved = resolve(&make_dep("https://example.com/repo.git", &upper)).unwrap();
        assert_eq!(resolved.sha, lower);
    }

    #[test]
    fn sha_passthrough_preserves_dep_fields() {
        let sha = "b".repeat(40);
        let mut dep = Dependency::new_git("my-addon", "https://example.com/repo.git", &sha);
        dep.map = Some(vec![MapEntry { from: "addons/foo".into(), to: None }]);
        let resolved = resolve(&dep).unwrap();
        assert_eq!(resolved.sha, sha);
        assert_eq!(resolved.dep.name, "my-addon");
        assert!(resolved.dep.map.is_some());
    }

    #[test]
    fn short_sha_not_treated_as_sha() {
        // 39-char hex string is not a full SHA - must go through remote resolution,
        // which will fail trying to connect to a non-existent host.
        let short = "a".repeat(39);
        let result = resolve(&make_dep("https://example.com/repo.git", &short));
        assert!(result.is_err());
    }

    #[test]
    fn looks_like_sha_requires_exactly_40_hex() {
        assert!( looks_like_sha(&"a".repeat(40)));
        assert!(!looks_like_sha(&"a".repeat(39)));
        assert!(!looks_like_sha(&"a".repeat(41)));
        assert!(!looks_like_sha("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"));
    }
}
