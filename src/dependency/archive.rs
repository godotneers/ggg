//! Downloading and extracting archive (zip / tar.gz) dependencies.
//!
//! [`download_and_extract`] downloads an archive URL to a temp file, verifies
//! its SHA-256 against the optional `sha256` field in the config, scans every
//! entry for path traversal attacks (rejecting the whole archive on any hit),
//! then extracts it into `dest_dir` as-is (no stripping).
//!
//! `strip_components` is intentionally NOT applied here.  It is applied later
//! at install-time (in [`super::install`]) so that changing the setting in
//! `ggg.toml` takes effect on the next `ggg sync` without re-downloading the
//! archive.
//!
//! The caller ([`super::cache::DependencyCache::install_archive`]) creates
//! `dest_dir` as a temp directory inside the cache hash directory, so the
//! final atomic rename stays on the same filesystem.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::{Dependency, DepKind};
use crate::utils::archive as archive_util;

/// Filename written into every cache entry for human inspection.
const METADATA_FILE: &str = ".ggg_dep_info.toml";

// ---------------------------------------------------------------------------
// Supported archive formats
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum ArchiveFormat {
    Zip,
    TarGz,
}

fn detect_format(url: &str) -> Result<ArchiveFormat> {
    if url.ends_with(".zip") {
        Ok(ArchiveFormat::Zip)
    } else if url.ends_with(".tar.gz") || url.ends_with(".tgz") {
        Ok(ArchiveFormat::TarGz)
    } else {
        bail!("unrecognised archive format in URL {url:?}; supported: .zip, .tar.gz, .tgz")
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Download the archive for `dep`, verify integrity, scan for path traversal,
/// extract into `dest_dir`, and write `.ggg_dep_info.toml`.
///
/// Returns the SHA-256 hex digest of the downloaded archive file.  The caller
/// uses this as the cache subdirectory name (the final piece of the cache key).
///
/// Files are extracted without any stripping; `strip_components` is applied
/// at install-time by [`super::install`].  All written files are made
/// read-only immediately after extraction.
pub fn download_and_extract(dep: &Dependency, dest_dir: &Path) -> Result<String> {
    let DepKind::Archive { url, sha256: config_sha256, .. } = dep.kind() else {
        bail!("download_and_extract called on non-archive dependency {:?}", dep.name);
    };

    let format = detect_format(url)?;

    // Step 1: download to a temp file while computing SHA-256.
    let (archive_sha, archive_path) = download_to_temp(url, &dep.name)
        .with_context(|| format!("failed to download {:?} from {url:?}", dep.name))?;

    // Step 2: verify the optional expected hash from ggg.toml.
    if let Some(expected) = config_sha256 {
        if archive_sha != expected {
            bail!(
                "SHA-256 mismatch for {:?}:\n  expected: {expected}\n  got:      {archive_sha}",
                dep.name
            );
        }
    }

    // Step 3: scan ALL entries for path traversal before extracting anything.
    let scan = match format {
        ArchiveFormat::Zip   => archive_util::scan_zip(&archive_path),
        ArchiveFormat::TarGz => archive_util::scan_tar_gz(&archive_path),
    };
    scan.with_context(|| format!("archive {:?} contains unsafe paths - refusing to extract", dep.name))?;

    // Step 4: extract into dest_dir as-is (no strip_components here).
    let extract = match format {
        ArchiveFormat::Zip   => archive_util::extract_zip(&archive_path, dest_dir),
        ArchiveFormat::TarGz => archive_util::extract_tar_gz(&archive_path, dest_dir),
    };
    extract.with_context(|| format!("failed to extract {:?}", dep.name))?;

    // Step 5: write metadata.
    write_metadata(dep, &archive_sha, dest_dir)
        .context("failed to write archive dependency metadata")?;

    // Clean up the temp archive file.
    let _ = std::fs::remove_file(&archive_path);

    Ok(archive_sha)
}

// ---------------------------------------------------------------------------
// Download
// ---------------------------------------------------------------------------

/// Stream `url` to a temp file, compute its SHA-256, return `(sha256_hex, path)`.
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
            let n = response
                .read(&mut buf)
                .context("error reading download response")?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            writer
                .write_all(&buf[..n])
                .context("failed to write archive to temp file")?;
            pb.inc(n as u64);
        }
        writer.flush().context("failed to flush archive temp file")?;
    } // writer dropped here, releasing the borrow on tmp

    pb.finish_and_clear();

    let archive_sha = format!("{:x}", hasher.finalize());
    let (_, path) = tmp
        .keep()
        .context("failed to persist temporary archive file")?;

    Ok((archive_sha, path))
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ArchiveDepInfo<'a> {
    name:        &'a str,
    url:         &'a str,
    archive_sha: &'a str,
}

fn write_metadata(dep: &Dependency, archive_sha: &str, dest_dir: &Path) -> Result<()> {
    let DepKind::Archive { url, .. } = dep.kind() else {
        bail!("write_metadata called on non-archive dep");
    };
    let info = ArchiveDepInfo { name: &dep.name, url, archive_sha };
    let content = toml_edit::ser::to_string_pretty(&info)
        .context("failed to serialize archive metadata")?;
    let path = dest_dir.join(METADATA_FILE);
    std::fs::write(&path, &content)
        .with_context(|| format!("failed to write {}", path.display()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Strip `n` leading components from `path_str`. Returns `None` if the
    /// path has `n` or fewer components.
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
    fn detect_format_zip() {
        assert!(matches!(detect_format("https://example.com/foo.zip").unwrap(), ArchiveFormat::Zip));
    }

    #[test]
    fn detect_format_tar_gz() {
        assert!(matches!(detect_format("https://example.com/foo.tar.gz").unwrap(), ArchiveFormat::TarGz));
        assert!(matches!(detect_format("https://example.com/foo.tgz").unwrap(), ArchiveFormat::TarGz));
    }

    #[test]
    fn detect_format_unknown_errors() {
        assert!(detect_format("https://example.com/foo.rar").is_err());
    }

    #[test]
    fn strip_components_zero() {
        assert_eq!(
            strip_components("a/b/c.txt", 0),
            Some(PathBuf::from("a/b/c.txt"))
        );
    }

    #[test]
    fn strip_components_one() {
        let result = strip_components("wrapper/addons/gut/plugin.cfg", 1).unwrap();
        assert_eq!(result, PathBuf::from("addons/gut/plugin.cfg"));
    }

    #[test]
    fn strip_components_skips_shallow_entries() {
        // Entry lives entirely within the stripped prefix.
        assert!(strip_components("wrapper", 1).is_none());
        assert!(strip_components("wrapper/", 1).is_none());
    }

    #[test]
    fn strip_components_two() {
        let result = strip_components("outer/inner/file.txt", 2).unwrap();
        assert_eq!(result, PathBuf::from("file.txt"));
    }
}
