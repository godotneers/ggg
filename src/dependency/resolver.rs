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

use anyhow::{Context, Result, bail};
use gix::bstr::ByteSlice;
use gix::progress::Discard;
use gix::protocol::handshake::Ref;

use crate::config::{Dependency, Source};
use crate::dependency::lockfile::{LockEntry, LockFile};
use crate::dependency::{
    ArchiveResolvedDependency, AssetLibResolvedDependency, AssetStoreResolvedDependency,
    GitResolvedDependency, ResolvedDependency, ResolvedMeta,
};
use crate::godot::asset_store::select_pinned_release;
use crate::godot::release::GodotVersion;

/// The kind-independent metadata every resolved dependency carries.
fn resolved_meta(dep: &Dependency) -> ResolvedMeta {
    ResolvedMeta {
        name: dep.name.clone(),
        map: dep.map.clone(),
        exclude: dep.exclude.clone(),
    }
}

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
    let Source::Git { git, rev } = &dep.source else {
        panic!("resolver::resolve() called on non-git dependency; check dep type first");
    };

    let sha = if looks_like_sha(rev) {
        rev.to_lowercase()
    } else {
        resolve_remote(git, rev)
            .with_context(|| format!("failed to resolve dependency {:?}", dep.name))?
    };
    Ok(ResolvedDependency::Git(GitResolvedDependency {
        meta: resolved_meta(dep),
        git: git.to_owned(),
        rev: rev.to_owned(),
        sha,
    }))
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
/// - **Asset Store**: URL + SHA + release identity from lock, or the release
///   list from the Asset Store API selected by the pinned version string (SHA
///   is empty until the archive is downloaded). `godot_version` is required
///   for store deps so the resolver can warn when the pinned release's
///   compatibility range does not cover the project's Godot version.
pub fn resolve_dependency(
    dep: &Dependency,
    lock: &LockFile,
    godot_version: Option<&GodotVersion>,
) -> Result<(ResolvedDependency, String)> {
    // Reuse the locked entry when it is a faithful snapshot of ggg.toml (no
    // drift) and complete enough to rebuild the resolved dependency. Anything
    // else - missing, drifted, or incomplete - resolves fresh below.
    if let Some(entry) = lock.verified_entry(dep) {
        return Ok(from_locked_entry(dep, entry));
    }

    match &dep.source {
        Source::Git { .. } => {
            let r = resolve(dep)
                .with_context(|| format!("failed to resolve dependency {:?}", dep.name))?;
            let note = format!("resolved {}", &r.sha()[..12]);
            Ok((r, note))
        }

        Source::Archive {
            url,
            sha256,
            strip_components,
        } => {
            // sha may be empty if no sha256 field; it will be computed on download.
            Ok((
                ResolvedDependency::Archive(ArchiveResolvedDependency {
                    meta: resolved_meta(dep),
                    url: url.to_owned(),
                    sha256: sha256.clone(),
                    strip_components: *strip_components,
                    sha: sha256.as_deref().unwrap_or("").to_owned(),
                }),
                String::new(),
            ))
        }

        Source::AssetLib {
            asset_library_id,
            strip_components,
        } => {
            let asset_library_id = *asset_library_id;
            let strip_components = *strip_components;

            // No lock entry - resolve the download URL via the asset library API.
            // The archive SHA is not known until the file is actually downloaded.
            println!("  {} - fetching from Godot Asset Library...", dep.name);
            let detail =
                crate::godot::asset_lib::get_asset(asset_library_id).with_context(|| {
                    format!(
                        "failed to fetch asset {:?} (id={}) from the Godot Asset Library",
                        dep.name, asset_library_id,
                    )
                })?;
            Ok((
                ResolvedDependency::AssetLib(AssetLibResolvedDependency {
                    meta: resolved_meta(dep),
                    asset_library_id,
                    strip_components,
                    resolved_url: detail.download_url,
                    sha: String::new(),
                    asset_version: Some(detail.version),
                }),
                format!("downloaded v{}", detail.version_string),
            ))
        }

        Source::AssetStore {
            asset_store_asset,
            strip_components,
        } => {
            let publisher = &asset_store_asset.publisher;
            let slug = &asset_store_asset.asset;
            let pinned_version = &asset_store_asset.version;
            let strip_components = *strip_components;

            // Resolve the pinned version against the asset's full release list.
            // No `compatibility` filter is sent - the release's range is checked
            // client-side below so ggg can warn (rather than silently drop) when
            // the pinned release does not cover the project's Godot version.
            let project_version = godot_version.with_context(|| {
                format!(
                    "dependency {:?}: resolving a Godot Asset Store dependency requires \
                     the project Godot version",
                    dep.name
                )
            })?;

            println!(
                "  {} - fetching {asset_store_asset} from the Godot Asset Store...",
                dep.name
            );
            let releases = crate::godot::asset_store::get_releases(publisher, slug, None, false)
                .with_context(|| {
                    format!(
                        "failed to fetch releases of {publisher}/{slug} from the Godot Asset Store"
                    )
                })?;

            let release = select_pinned_release(&releases, pinned_version).with_context(|| {
                format!(
                    "dependency {:?}: release {:?} of {publisher}/{slug} was not found in \
                     the Godot Asset Store",
                    dep.name, pinned_version
                )
            })?;

            if !release.is_compatible_with(project_version) {
                let min = format!("v{}", release.min_godot_version);
                let max = release
                    .max_godot_version
                    .as_deref()
                    .map(|v| format!("v{v}"))
                    .unwrap_or_else(|| "latest".to_string());
                eprintln!(
                    "  warning: {publisher}/{slug} v{} requires Godot {min}..{max}, but the \
                     project uses Godot v{project_version}; installing anyway",
                    release.version,
                );
            }

            Ok((
                ResolvedDependency::AssetStore(AssetStoreResolvedDependency {
                    meta: resolved_meta(dep),
                    publisher: publisher.to_owned(),
                    asset: slug.to_owned(),
                    version: pinned_version.to_owned(),
                    strip_components,
                    resolved_url: release.download_url.clone(),
                    sha: String::new(),
                    release_id: Some(release.id),
                    release_version: Some(release.version.clone()),
                }),
                format!("downloaded v{}", release.version),
            ))
        }
    }
}

/// Rebuild a [`ResolvedDependency`] from a lock entry that
/// [`LockFile::verified_entry`] has already checked for drift and
/// completeness, together with the human-readable note describing the locked
/// version.
fn from_locked_entry(dep: &Dependency, entry: &LockEntry) -> (ResolvedDependency, String) {
    match &dep.source {
        Source::Git { git, rev } => {
            let sha = entry
                .sha
                .as_deref()
                .expect("verified_entry guarantees a git sha");
            (
                ResolvedDependency::Git(GitResolvedDependency {
                    meta: resolved_meta(dep),
                    git: git.to_owned(),
                    rev: rev.to_owned(),
                    sha: sha.to_owned(),
                }),
                format!("locked {}", &sha[..12]),
            )
        }

        Source::Archive {
            url,
            sha256,
            strip_components,
        } => {
            let sha = entry
                .archive_sha
                .as_deref()
                .expect("verified_entry guarantees an archive sha");
            (
                ResolvedDependency::Archive(ArchiveResolvedDependency {
                    meta: resolved_meta(dep),
                    url: url.to_owned(),
                    sha256: sha256.clone(),
                    strip_components: *strip_components,
                    sha: sha.to_owned(),
                }),
                format!("locked {}", &sha[..8]),
            )
        }

        Source::AssetLib {
            asset_library_id,
            strip_components,
        } => {
            let url = entry
                .url
                .as_deref()
                .expect("verified_entry guarantees an asset url");
            let sha = entry
                .archive_sha
                .as_deref()
                .expect("verified_entry guarantees an asset archive sha");
            let version_label = entry
                .asset_version
                .map(|v| format!(" (version {})", v))
                .unwrap_or_default();
            (
                ResolvedDependency::AssetLib(AssetLibResolvedDependency {
                    meta: resolved_meta(dep),
                    asset_library_id: *asset_library_id,
                    strip_components: *strip_components,
                    resolved_url: url.to_owned(),
                    sha: sha.to_owned(),
                    asset_version: entry.asset_version,
                }),
                format!("locked{version_label}"),
            )
        }

        Source::AssetStore {
            asset_store_asset,
            strip_components,
        } => {
            let url = entry
                .url
                .as_deref()
                .expect("verified_entry guarantees a store url");
            let sha = entry
                .archive_sha
                .as_deref()
                .expect("verified_entry guarantees a store archive sha");
            let version_label = entry
                .release_version
                .as_deref()
                .map(|v| format!(" v{v}"))
                .unwrap_or_default();
            (
                ResolvedDependency::AssetStore(AssetStoreResolvedDependency {
                    meta: resolved_meta(dep),
                    publisher: asset_store_asset.publisher.to_owned(),
                    asset: asset_store_asset.asset.to_owned(),
                    version: asset_store_asset.version.to_owned(),
                    strip_components: *strip_components,
                    resolved_url: url.to_owned(),
                    sha: sha.to_owned(),
                    release_id: entry.release_id,
                    release_version: entry.release_version.clone(),
                }),
                format!("locked{version_label}"),
            )
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
    let refs =
        list_remote_refs(url).with_context(|| format!("failed to list refs from {url:?}"))?;

    // Try candidates in priority order:
    // 1. Annotated tag (Peeled) - most specific, `object` is the commit
    // 2. Direct ref matching refs/tags/<rev>  (lightweight tag)
    // 3. Direct ref matching refs/heads/<rev> (branch)
    // 4. Any Direct ref whose name exactly matches rev (e.g. "HEAD")
    let tag_ref = format!("refs/tags/{rev}");
    let head_ref = format!("refs/heads/{rev}");

    // Annotated tag: server sends a Peeled entry; `object` is the commit SHA.
    for r in &refs {
        if let Ref::Peeled {
            full_ref_name,
            object,
            ..
        } = r
            && full_ref_name.as_bstr() == tag_ref.as_bytes().as_bstr()
        {
            return Ok(object.to_hex().to_string());
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
        Ref::Direct {
            full_ref_name,
            object,
        } if full_ref_name.as_bstr() == name.as_bytes().as_bstr() => {
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
    let repo = gix::init_bare(tmp.path()).context("failed to initialise temporary repository")?;

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
        .with_refspecs(["+refs/*:refs/*"], gix::remote::Direction::Fetch)
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
        Dependency::new(
            "test",
            Source::Git {
                git: git.to_owned(),
                rev: rev.to_owned(),
            },
            None,
            None,
        )
    }

    #[test]
    fn sha_passthrough_lowercase() {
        let sha = "a".repeat(40);
        let resolved = resolve(&make_dep("https://example.com/repo.git", &sha)).unwrap();
        assert_eq!(resolved.sha(), sha);
    }

    #[test]
    fn sha_passthrough_uppercase_normalised() {
        let upper = "A".repeat(40);
        let lower = "a".repeat(40);
        let resolved = resolve(&make_dep("https://example.com/repo.git", &upper)).unwrap();
        assert_eq!(resolved.sha(), lower);
    }

    #[test]
    fn sha_passthrough_preserves_dep_fields() {
        let sha = "b".repeat(40);
        let dep = Dependency::new(
            "my-addon",
            Source::Git {
                git: "https://example.com/repo.git".to_owned(),
                rev: sha.clone(),
            },
            Some(vec![MapEntry {
                from: "addons/foo".into(),
                to: None,
            }]),
            None,
        );
        let resolved = resolve(&dep).unwrap();
        assert_eq!(resolved.sha(), sha);
        assert_eq!(resolved.name(), "my-addon");
        assert!(resolved.map().is_some());
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
        assert!(looks_like_sha(&"a".repeat(40)));
        assert!(!looks_like_sha(&"a".repeat(39)));
        assert!(!looks_like_sha(&"a".repeat(41)));
        assert!(!looks_like_sha("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"));
    }

    // --- lock reuse ----------------------------------------------------------

    #[test]
    fn reuse_uses_locked_git_sha_without_network() {
        let sha = "a".repeat(40);
        let mut lock = LockFile::default();
        lock.upsert(&ResolvedDependency::Git(GitResolvedDependency {
            meta: ResolvedMeta {
                name: "gut".into(),
                map: None,
                exclude: None,
            },
            git: "https://example.com/repo.git".into(),
            rev: "main".into(),
            sha: sha.clone(),
        }));

        let dep = Dependency::new(
            "gut",
            Source::Git {
                git: "https://example.com/repo.git".into(),
                rev: "main".into(),
            },
            None,
            None,
        );
        let (resolved, note) = resolve_dependency(&dep, &lock, None).unwrap();
        assert_eq!(resolved.sha(), sha);
        assert_eq!(note, format!("locked {}", &sha[..12]));
    }

    #[test]
    fn reuse_ignores_locked_entry_when_rev_edited_in_config() {
        let mut lock = LockFile::default();
        lock.upsert(&ResolvedDependency::Git(GitResolvedDependency {
            meta: ResolvedMeta {
                name: "gut".into(),
                map: None,
                exclude: None,
            },
            git: "https://example.com/repo.git".into(),
            rev: "v1.0.0".into(),
            sha: "a".repeat(40),
        }));

        let dep = Dependency::new(
            "gut",
            Source::Git {
                git: "https://example.com/repo.git".into(),
                rev: "main".into(),
            },
            None,
            None,
        );
        // The edited rev is not in the lock, so resolution falls through to the
        // remote and fails offline rather than silently reusing the stale sha.
        assert!(resolve_dependency(&dep, &lock, None).is_err());
    }

    #[test]
    fn reuse_uses_locked_archive_sha_and_keeps_config_sha256() {
        let sha = "e3b0c44298fc1c149afbf4c8996fb924".repeat(2);
        let mut lock = LockFile::default();
        lock.upsert(&ResolvedDependency::Archive(ArchiveResolvedDependency {
            meta: ResolvedMeta {
                name: "foo".into(),
                map: None,
                exclude: None,
            },
            url: "https://example.com/foo.zip".into(),
            sha256: None,
            strip_components: None,
            sha: sha.clone(),
        }));

        let config_sha256 = Some("f".repeat(64));
        let dep = Dependency::new(
            "foo",
            Source::Archive {
                url: "https://example.com/foo.zip".into(),
                sha256: config_sha256.clone(),
                strip_components: None,
            },
            None,
            None,
        );
        let (resolved, note) = resolve_dependency(&dep, &lock, None).unwrap();
        let ResolvedDependency::Archive(a) = resolved else {
            panic!("expected an archive dependency");
        };
        assert_eq!(a.sha, sha);
        assert_eq!(a.sha256, config_sha256);
        assert_eq!(note, format!("locked {}", &sha[..8]));
    }

    #[test]
    fn reuse_builds_asset_lib_from_locked_entry() {
        let sha = "e3b0c44298fc1c149afbf4c8996fb924".repeat(2);
        let mut lock = LockFile::default();
        lock.upsert(&ResolvedDependency::AssetLib(AssetLibResolvedDependency {
            meta: ResolvedMeta {
                name: "dialogic".into(),
                map: None,
                exclude: None,
            },
            asset_library_id: 1216,
            strip_components: None,
            resolved_url: "https://example.com/dialogic.zip".into(),
            sha: sha.clone(),
            asset_version: Some(3),
        }));

        let dep = Dependency::new(
            "dialogic",
            Source::AssetLib {
                asset_library_id: 1216,
                strip_components: None,
            },
            None,
            None,
        );
        let (resolved, note) = resolve_dependency(&dep, &lock, None).unwrap();
        let ResolvedDependency::AssetLib(a) = resolved else {
            panic!("expected an asset library dependency");
        };
        assert_eq!(a.resolved_url, "https://example.com/dialogic.zip");
        assert_eq!(a.sha, sha);
        assert_eq!(a.asset_version, Some(3));
        assert_eq!(note, "locked (version 3)");
    }

    #[test]
    fn reuse_builds_store_dep_from_locked_entry() {
        let sha = "e3b0c44298fc1c149afbf4c8996fb924".repeat(2);
        let mut lock = LockFile::default();
        lock.upsert(&ResolvedDependency::AssetStore(
            AssetStoreResolvedDependency {
                meta: ResolvedMeta {
                    name: "my-addon".into(),
                    map: None,
                    exclude: None,
                },
                publisher: "souleat".into(),
                asset: "my-addon".into(),
                version: "1.2.3".into(),
                strip_components: None,
                resolved_url: "https://example.com/souleat-my-addon-1.2.3.zip".into(),
                sha: sha.clone(),
                release_id: Some(42),
                release_version: Some("1.2.3".into()),
            },
        ));

        let dep = Dependency::new(
            "my-addon",
            Source::AssetStore {
                asset_store_asset: "souleat/my-addon:1.2.3".parse().unwrap(),
                strip_components: None,
            },
            None,
            None,
        );
        let (resolved, note) = resolve_dependency(&dep, &lock, None).unwrap();
        let ResolvedDependency::AssetStore(a) = resolved else {
            panic!("expected an asset store dependency");
        };
        assert_eq!(
            a.resolved_url,
            "https://example.com/souleat-my-addon-1.2.3.zip"
        );
        assert_eq!(a.sha, sha);
        assert_eq!(a.release_id, Some(42));
        assert_eq!(a.release_version.as_deref(), Some("1.2.3"));
        assert_eq!(note, "locked v1.2.3");
    }

    #[test]
    fn reuse_skips_store_entry_when_pinned_version_changed() {
        let mut lock = LockFile::default();
        lock.upsert(&ResolvedDependency::AssetStore(
            AssetStoreResolvedDependency {
                meta: ResolvedMeta {
                    name: "my-addon".into(),
                    map: None,
                    exclude: None,
                },
                publisher: "souleat".into(),
                asset: "my-addon".into(),
                version: "1.2.3".into(),
                strip_components: None,
                resolved_url: "https://example.com/souleat-my-addon-1.2.3.zip".into(),
                sha: "e3b0c44298fc1c149afbf4c8996fb924".repeat(2),
                release_id: Some(42),
                release_version: Some("1.2.3".into()),
            },
        ));

        let dep = Dependency::new(
            "my-addon",
            Source::AssetStore {
                asset_store_asset: "souleat/my-addon:2.0.0".parse().unwrap(),
                strip_components: None,
            },
            None,
            None,
        );
        // Fresh resolution needs the project Godot version; with it missing the
        // stale locked entry must not be reused.
        assert!(resolve_dependency(&dep, &lock, None).is_err());
    }
}
