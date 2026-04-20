//! Shared archive scanning and extraction utilities.
//!
//! Both Godot release archives and dependency archives go through the same
//! two-phase pipeline:
//!
//! 1. **Scan** ([`scan_zip`] / [`scan_tar_gz`]) - walk every entry and reject
//!    the whole archive if any path is unsafe.  No files are written yet.
//! 2. **Extract** ([`extract_zip`] / [`extract_tar_gz`]) - write files to
//!    disk, filter `__MACOSX` junk, and mark every file read-only.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};


// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

/// Scan every entry in a zip archive for unsafe paths.
///
/// If ANY entry has an unsafe path the whole archive is rejected - no files
/// are written.
pub fn scan_zip(archive_path: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("failed to open archive {}", archive_path.display()))?;
    let mut zip = zip::ZipArchive::new(file).context("failed to read zip archive")?;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).context("failed to read zip entry")?;
        check_path(entry.name()).with_context(|| format!("unsafe path in zip entry {i}"))?;
    }
    Ok(())
}

/// Scan every entry in a tar.gz archive for unsafe paths.
///
/// If ANY entry has an unsafe path the whole archive is rejected - no files
/// are written.
pub fn scan_tar_gz(archive_path: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("failed to open archive {}", archive_path.display()))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    for (i, entry) in archive.entries().context("failed to read tar entries")?.enumerate() {
        let entry = entry.with_context(|| format!("failed to read tar entry {i}"))?;
        let path = entry
            .path()
            .with_context(|| format!("failed to read path for tar entry {i}"))?;
        check_path(&path.to_string_lossy())
            .with_context(|| format!("unsafe path in tar entry {i}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/// Extract a zip archive into `dest_dir`.
///
/// - Skips `__MACOSX` metadata entries produced by macOS zip tools.
/// - Skips directory entries; parent directories are created on demand.
/// - Marks every extracted file read-only.
pub fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("failed to open archive {}", archive_path.display()))?;
    let mut zip = zip::ZipArchive::new(file).context("failed to read zip archive")?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).context("failed to read zip entry")?;

        if entry.name().contains("__MACOSX") || entry.is_dir() {
            continue;
        }

        let rel = PathBuf::from(entry.name());
        if rel.as_os_str().is_empty() {
            continue;
        }

        write_entry(&rel, &mut entry, dest_dir)
            .with_context(|| format!("failed to extract {}", rel.display()))?;
    }
    Ok(())
}

/// Extract a tar.gz archive into `dest_dir`.
///
/// - Skips directory entries; parent directories are created on demand.
/// - Marks every extracted file read-only.
pub fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("failed to open archive {}", archive_path.display()))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);

    for (i, entry) in archive.entries().context("failed to read tar entries")?.enumerate() {
        let mut entry = entry.with_context(|| format!("failed to read tar entry {i}"))?;

        if entry.header().entry_type().is_dir() {
            continue;
        }

        let rel = entry
            .path()
            .with_context(|| format!("failed to read path for tar entry {i}"))?
            .into_owned();

        if rel.as_os_str().is_empty() {
            continue;
        }

        write_entry(&rel, &mut entry, dest_dir)
            .with_context(|| format!("failed to extract {}", rel.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Check that `path_str` contains no absolute paths or `..` components.
///
/// Called on every archive entry before extraction begins.  Does NOT check
/// post-strip paths - raw entry paths are validated as-is.
fn check_path(path_str: &str) -> Result<()> {
    if path_str.starts_with('/') || path_str.starts_with('\\') {
        bail!("absolute path in archive: {path_str:?}");
    }
    for component in Path::new(path_str).components() {
        if component == Component::ParentDir {
            bail!("path traversal in archive: {path_str:?}");
        }
        // Reject drive-letter prefixes like C: on Windows.
        if let Component::Prefix(_) = component {
            bail!("absolute path prefix in archive: {path_str:?}");
        }
    }
    Ok(())
}

/// Write a single file entry to `dest_dir/rel`, creating parent directories
/// as needed, then mark the file read-only.
fn write_entry(rel: &Path, reader: &mut dyn Read, dest_dir: &Path) -> Result<()> {
    let dest_path = dest_dir.join(rel);
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    let mut out = std::fs::File::create(&dest_path)
        .with_context(|| format!("failed to create {}", dest_path.display()))?;
    std::io::copy(reader, &mut out)
        .with_context(|| format!("failed to write {}", dest_path.display()))?;
    drop(out);
    make_readonly(&dest_path)
}

fn make_readonly(path: &Path) -> Result<()> {
    let mut perms = std::fs::metadata(path)
        .with_context(|| format!("failed to read permissions of {}", path.display()))?
        .permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("failed to set read-only on {}", path.display()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(&mut buf);
        for (name, content) in entries {
            w.start_file::<_, ()>(*name, Default::default()).unwrap();
            w.write_all(content).unwrap();
        }
        w.finish().unwrap();
        buf.into_inner()
    }

    #[test]
    fn check_path_accepts_normal_paths() {
        assert!(check_path("addons/gut/plugin.cfg").is_ok());
        assert!(check_path("README.md").is_ok());
        assert!(check_path("a/b/c/d.txt").is_ok());
    }

    #[test]
    fn check_path_rejects_absolute_unix() {
        assert!(check_path("/etc/passwd").is_err());
    }

    #[test]
    fn check_path_rejects_parent_dir() {
        assert!(check_path("../../etc/passwd").is_err());
        assert!(check_path("a/../../../etc/passwd").is_err());
    }

    #[test]
    fn scan_zip_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evil.zip");
        std::fs::write(&path, make_zip(&[("../../evil.txt", b"pwned")])).unwrap();

        let err = format!("{:#}", scan_zip(&path).unwrap_err());
        assert!(err.contains("path traversal"), "unexpected error: {err}");
    }

    #[test]
    fn scan_zip_accepts_safe_archive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ok.zip");
        std::fs::write(&path, make_zip(&[("addons/gut/plugin.cfg", b"# gut")])).unwrap();
        assert!(scan_zip(&path).is_ok());
    }

    #[test]
    fn extract_zip_writes_files_and_creates_parents() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        std::fs::write(&archive, make_zip(&[("sub/file.txt", b"hello")])).unwrap();
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();

        extract_zip(&archive, &dest).unwrap();

        assert_eq!(std::fs::read(dest.join("sub/file.txt")).unwrap(), b"hello");
    }

    #[test]
    fn extract_zip_skips_macosx_entries() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        std::fs::write(&archive, make_zip(&[
            ("__MACOSX/._something", b"junk"),
            ("real.txt", b"real"),
        ])).unwrap();
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();

        extract_zip(&archive, &dest).unwrap();

        assert!(dest.join("real.txt").exists());
        assert!(!dest.join("__MACOSX").exists());
    }

    #[test]
    fn extracted_files_are_readonly() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        std::fs::write(&archive, make_zip(&[("file.txt", b"content")])).unwrap();
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();

        extract_zip(&archive, &dest).unwrap();

        let perms = std::fs::metadata(dest.join("file.txt")).unwrap().permissions();
        assert!(perms.readonly());
    }
}
