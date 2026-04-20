//! Downloading dependency artifacts to temporary locations.
//!
//! [`download`] is the unified entry point: given a [`ResolvedDependency`] it
//! fetches the artifact to a temporary path and returns `(path, sha)`.
//!
//! - **Git** deps are cloned as bare repositories into a temp directory; `sha`
//!   is taken from the already-resolved [`ResolvedDependency`].
//! - **Archive** and **Asset Library** deps are streamed to a temp file while
//!   computing the SHA-256 digest.
//!
//! [`cleanup`] removes the temp artifact returned by [`download`].

use std::io::{Read, Write};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};

use crate::config::DepKind;
use crate::dependency::ResolvedDependency;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Download the artifact for `dep` to a temporary location.
///
/// Returns `(path, sha)`:
/// - Git: `path` is a temporary bare clone directory; `sha` is `dep.sha`.
/// - Archive/AssetLib: `path` is a temporary archive file; `sha` is the
///   computed SHA-256 of that file.
///
/// Call [`cleanup`] on the returned path after the cache has installed it.
pub fn download(dep: &ResolvedDependency) -> Result<(PathBuf, String)> {
    match dep.dep.kind() {
        DepKind::Git { git, .. } => {
            let path = download_git(&dep.dep.name, git, &dep.sha)?;
            Ok((path, dep.sha.clone()))
        }
        DepKind::Archive { url, sha256, .. } => {
            let sha_hint = if dep.sha.is_empty() { sha256 } else { Some(dep.sha.as_str()) };
            download_archive(&dep.dep.name, url, sha_hint)
        }
        DepKind::AssetLib { .. } => {
            let url = dep.resolved_url.as_deref()
                .expect("AssetLib ResolvedDependency must have resolved_url set");
            let sha_hint = if dep.sha.is_empty() { None } else { Some(dep.sha.as_str()) };
            download_archive(&dep.dep.name, url, sha_hint)
        }
    }
}

/// Remove the temporary artifact returned by [`download`].
pub fn cleanup(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

// ---------------------------------------------------------------------------
// Git
// ---------------------------------------------------------------------------

fn download_git(name: &str, url: &str, sha: &str) -> Result<PathBuf> {
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

    if let Err(e) = fetch(&repo, url, sha, Some(NonZeroU32::new(1).unwrap())) {
        pb.set_message(format!(
            "Shallow fetch unavailable ({e:#}), fetching full history for {name}"
        ));
        fetch(&repo, url, sha, None)
            .with_context(|| format!("failed to fetch dependency {name:?} from {url}"))?;
    }

    pb.finish_with_message(format!("Fetched {name}"));

    Ok(tmp.keep())
}

fn fetch(
    repo: &gix::Repository,
    url: &str,
    sha: &str,
    depth: Option<NonZeroU32>,
) -> Result<()> {
    let url_parsed = gix::url::parse(url.as_bytes().into())
        .with_context(|| format!("invalid git URL: {url:?}"))?;

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
        None    => prepare,
    };

    prepare
        .receive(gix::progress::Discard, &should_interrupt)
        .context("fetch failed")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Archive / AssetLib
// ---------------------------------------------------------------------------

fn download_archive(
    dep_name: &str,
    url: &str,
    sha_hint: Option<&str>,
) -> Result<(PathBuf, String)> {
    let (sha, path) = download_to_temp(url, dep_name)
        .with_context(|| format!("failed to download {dep_name:?} from {url:?}"))?;

    if let Some(expected) = sha_hint {
        if sha != expected {
            let _ = std::fs::remove_file(&path);
            bail!(
                "SHA-256 mismatch for {dep_name:?}:\n  expected: {expected}\n  got:      {sha}",
            );
        }
    }

    Ok((path, sha))
}

/// Stream `url` to a named temp file, compute SHA-256, return `(sha256_hex, path)`.
fn download_to_temp(url: &str, name: &str) -> Result<(String, PathBuf)> {
    let tmp = tempfile::NamedTempFile::new()
        .context("failed to create temporary file for archive download")?;

    let client = reqwest::blocking::Client::builder()
        .build()
        .context("failed to build HTTP client")?;

    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to GET {url:?}"))?
        .error_for_status()
        .with_context(|| format!("server returned error for {url:?}"))?;

    let content_length = response.content_length();

    let pb = ProgressBar::new(content_length.unwrap_or(0));
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.dim} Downloading {msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=>-"),
    );
    if content_length.is_none() {
        pb.set_style(
            ProgressStyle::with_template("{spinner:.dim} Downloading {msg} {bytes}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
    }
    pb.set_message(name.to_owned());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];

    {
        let mut writer = std::io::BufWriter::new(tmp.as_file());
        loop {
            let n = response.read(&mut buf).context("error reading download response")?;
            if n == 0 { break; }
            hasher.update(&buf[..n]);
            writer.write_all(&buf[..n]).context("failed to write archive to temp file")?;
            pb.inc(n as u64);
        }
        writer.flush().context("failed to flush archive temp file")?;
    }

    pb.finish_and_clear();

    let sha = format!("{:x}", hasher.finalize());
    let (_, path) = tmp.keep().context("failed to persist temporary archive file")?;

    Ok((sha, path))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_components(path_str: &str, n: u32) -> Option<PathBuf> {
        if n == 0 {
            return Some(PathBuf::from(path_str));
        }
        let components: Vec<_> = Path::new(path_str).components().collect();
        if components.len() <= n as usize {
            return None;
        }
        Some(components[n as usize..].iter().collect())
    }

    #[test]
    fn strip_components_zero() {
        assert_eq!(strip_components("a/b/c.txt", 0), Some(PathBuf::from("a/b/c.txt")));
    }

    #[test]
    fn strip_components_one() {
        let result = strip_components("wrapper/addons/gut/plugin.cfg", 1).unwrap();
        assert_eq!(result, PathBuf::from("addons/gut/plugin.cfg"));
    }

    #[test]
    fn strip_components_skips_shallow_entries() {
        assert!(strip_components("wrapper", 1).is_none());
        assert!(strip_components("wrapper/", 1).is_none());
    }

    #[test]
    fn strip_components_two() {
        let result = strip_components("outer/inner/file.txt", 2).unwrap();
        assert_eq!(result, PathBuf::from("file.txt"));
    }
}
