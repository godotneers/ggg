//! Downloading a dependency's git repository as a bare clone.
//!
//! [`download_dependency`] fetches the exact commit SHA recorded in a
//! [`ResolvedDependency`] into a new temporary bare repository and returns
//! the path to that directory.
//!
//! A shallow clone (depth=1) is attempted first to keep download times short.
//! If the server does not support fetching by commit SHA
//! (`allow-reachable-sha1-in-want`), the function retries with a full
//! (non-shallow) fetch as a fallback.
//!
//! The caller is responsible for moving the result into the dependency cache
//! and for cleaning up the temporary directory on error.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};

use crate::dependency::ResolvedDependency;

/// Download the git repository for `dep` as a bare clone into a new temporary
/// directory and return its path.
///
/// The caller is responsible for moving the directory into the dependency
/// cache and for cleaning it up on error.
pub fn download_dependency(dep: &ResolvedDependency) -> Result<PathBuf> {
    let url = &dep.dep.git;
    let sha = &dep.sha;
    let name = &dep.dep.name;

    let tmp = tempfile::TempDir::new()
        .context("failed to create temporary directory for dependency download")?;

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.dim} {msg}")?
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
    );
    pb.set_message(format!("Fetching {name}"));
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    let repo = gix::init_bare(tmp.path())
        .context("failed to initialise temporary bare repository")?;

    // Try shallow first; fall back to a full fetch if the server does not
    // support allow-reachable-sha1-in-want.
    if let Err(e) = fetch(&repo, url, sha, Some(NonZeroU32::new(1).unwrap())) {
        pb.set_message(format!(
            "Shallow fetch unavailable ({e:#}), fetching full history for {name}"
        ));
        fetch(&repo, url, sha, None)
            .with_context(|| format!("failed to fetch dependency {name:?} from {url}"))?;
    }

    pb.finish_with_message(format!("Fetched {name}"));

    // Consume the TempDir handle without deleting the directory on disk.
    Ok(tmp.keep())
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Fetch `sha` from `url` into the bare `repo`.
///
/// If `depth` is `Some`, a shallow fetch at that depth is requested.
fn fetch(
    repo: &gix::Repository,
    url: &str,
    sha: &str,
    depth: Option<NonZeroU32>,
) -> Result<()> {
    let url_parsed = gix::url::parse(url.as_bytes().into())
        .with_context(|| format!("invalid git URL: {url:?}"))?;

    // Fetch the specific commit SHA into a known local ref.
    let refspec = format!("{sha}:refs/ggg/fetched");

    let remote = repo
        .remote_at(url_parsed)
        .context("failed to configure remote")?
        .with_refspecs([refspec.as_str()], gix::remote::Direction::Fetch)
        .context("failed to configure fetch refspec")?;

    let should_interrupt = AtomicBool::new(false);

    let prepare = remote
        .connect(gix::remote::Direction::Fetch)
        .context("failed to connect to remote")?
        .prepare_fetch(gix::progress::Discard, Default::default())
        .context("failed to prepare fetch")?;

    let prepare = match depth {
        Some(d) => prepare.with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(d)),
        None => prepare,
    };

    prepare
        .receive(gix::progress::Discard, &should_interrupt)
        .context("fetch failed")?;

    Ok(())
}
