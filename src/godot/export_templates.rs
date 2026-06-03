//! Downloading, caching, and installing Godot export templates.
//!
//! Export templates are needed to publish games from the Godot editor.
//! ggg caches them under `{cache_root}/export_templates/{release_key}/` and
//! installs them into Godot's platform-specific data directory so the editor
//! finds them automatically.
//!
//! # Cache layout
//!
//! The downloaded `.tpz` (a standard zip) is extracted verbatim:
//!
//! ```text
//! {cache_root}/export_templates/4.3-stable/templates/version.txt
//!                                                    android/...
//!                                                    ...
//! ```
//!
//! # Install layout
//!
//! The `templates/` prefix is stripped when copying to Godot's data dir:
//!
//! ```text
//! {godot_data}/export_templates/4.3.stable/version.txt   (Godot 4.x)
//! {godot_data}/templates/3.5.stable/version.txt          (Godot 3.x)
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::release::GodotRelease;
use crate::cache::resolve_cache_root;
use crate::dependency::download::download_to_temp;

// ---------------------------------------------------------------------------
// URL builder
// ---------------------------------------------------------------------------

fn template_url(release: &GodotRelease) -> String {
    let slug = if release.mono {
        "mono_export_templates.tpz"
    } else {
        "export_templates.tpz"
    };
    format!(
        "https://downloads.godotengine.org/?version={}&flavor={}&slug={}&platform=templates",
        release.version, release.flavor, slug
    )
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

/// Manages the on-disk cache of extracted Godot export template archives.
pub struct ExportTemplateCache {
    root: PathBuf, // {cache_root}/export_templates/
}

impl ExportTemplateCache {
    /// Create a cache rooted at an explicit path.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Resolve the cache location from the environment and create a cache
    /// rooted there.
    pub fn from_env() -> Result<Self> {
        Ok(Self::new(resolve_cache_root()?.join("export_templates")))
    }

    /// The directory where a release's extracted template files are stored.
    pub fn dir(&self, release: &GodotRelease) -> PathBuf {
        self.root.join(release.cache_key())
    }

    /// Returns `true` if this release's templates are fully extracted in the cache.
    ///
    /// Presence of `templates/version.txt` is the canonical completion marker.
    pub fn contains(&self, release: &GodotRelease) -> bool {
        self.dir(release).join("templates").join("version.txt").exists()
    }

    /// Extract a downloaded `.tpz` archive into the cache directory.
    ///
    /// Any previous (possibly partial) contents are removed first.
    pub fn install(&self, release: &GodotRelease, archive: &Path) -> Result<()> {
        release.validate()?;

        let dir = self.dir(release);

        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| {
                format!("failed to remove existing cache at {}", dir.display())
            })?;
        }
        std::fs::create_dir_all(&dir).with_context(|| {
            format!("failed to create cache directory {}", dir.display())
        })?;

        crate::utils::archive::scan_zip(archive)
            .context("export template archive contains unsafe paths - refusing to extract")?;
        crate::utils::archive::extract_zip(archive, &dir)?;

        Ok(())
    }

    /// Remove a cached release, freeing its disk space.
    ///
    /// Does nothing if the release is not cached.
    pub fn remove(&self, release: &GodotRelease) -> Result<()> {
        release.validate()?;

        let dir = self.dir(release);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .with_context(|| format!("failed to remove {}", dir.display()))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Godot install path helpers
// ---------------------------------------------------------------------------

/// Format the version string Godot uses for its export template directory.
///
/// Maps ggg's dash-separated cache key to Godot's dot-separated format:
/// `"4.3-stable"` -> `"4.3.stable"`, `"4.3-stable-mono"` -> `"4.3.stable.mono"`.
fn godot_install_version(release: &GodotRelease) -> String {
    let base = format!("{}.{}", release.version, release.flavor);
    if release.mono {
        format!("{}.mono", base)
    } else {
        base
    }
}

/// The subdirectory name Godot uses inside its data dir for templates.
///
/// Godot 4.x uses `export_templates/`; Godot 3.x uses `templates/`.
fn godot_templates_subdir(release: &GodotRelease) -> &'static str {
    if release.version.major >= 4 {
        "export_templates"
    } else {
        "templates"
    }
}

/// Platform-specific base directory where Godot stores its editor data,
/// joined with the appropriate templates subdirectory.
fn godot_templates_dir(release: &GodotRelease) -> Result<PathBuf> {
    let data = dirs::data_dir().context("could not determine platform data directory")?;

    // Linux uses lowercase "godot"; macOS and Windows use "Godot"
    #[cfg(target_os = "linux")]
    let godot_dir = data.join("godot");
    #[cfg(not(target_os = "linux"))]
    let godot_dir = data.join("Godot");

    Ok(godot_dir.join(godot_templates_subdir(release)))
}

/// The directory where Godot expects to find the templates for `release`.
fn template_install_path(release: &GodotRelease) -> Result<PathBuf> {
    Ok(godot_templates_dir(release)?.join(godot_install_version(release)))
}

// ---------------------------------------------------------------------------
// Recursive directory copy
// ---------------------------------------------------------------------------

/// Copy all files from `src` into `dst`, creating directories as needed.
///
/// Destination files are writable (unlike the read-only cached source files).
fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)
        .with_context(|| format!("failed to create directory {}", dst.display()))?;
    for entry in std::fs::read_dir(src)
        .with_context(|| format!("failed to read directory {}", src.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", src.display()))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry
            .file_type()
            .with_context(|| format!("failed to get file type for {}", src_path.display()))?
            .is_dir()
        {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
            // Ensure destination files are writable. std::fs::copy preserves
            // the source readonly flag on Windows, but Godot needs to read
            // these files normally and users may want to inspect them.
            let mut perms = std::fs::metadata(&dst_path)
                .with_context(|| {
                    format!("failed to read permissions of {}", dst_path.display())
                })?
                .permissions();
            perms.set_readonly(false);
            std::fs::set_permissions(&dst_path, perms).with_context(|| {
                format!("failed to set permissions on {}", dst_path.display())
            })?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Download (if not cached) and install export templates for `release`.
///
/// Skips all work if `version.txt` is already present in Godot's install dir,
/// which means templates were installed either by ggg or by Godot's own
/// template manager.
pub fn ensure_export_templates(release: &GodotRelease) -> Result<()> {
    let install_path = template_install_path(release)?;
    if install_path.join("version.txt").exists() {
        return Ok(());
    }

    let cache = ExportTemplateCache::from_env()?;

    if !cache.contains(release) {
        let url = template_url(release);
        let (_sha, tmp_path) =
            download_to_temp(&url, &format!("export templates {}", release))
                .with_context(|| {
                    format!("failed to download export templates for {}", release)
                })?;
        let result = cache.install(release, &tmp_path);
        let _ = std::fs::remove_file(&tmp_path);
        result?;
    }

    let templates_src = cache.dir(release).join("templates");
    copy_dir_all(&templates_src, &install_path).with_context(|| {
        format!(
            "failed to install export templates to {}",
            install_path.display()
        )
    })?;

    println!("Installed export templates for {}", release);

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::godot::release::GodotVersion;

    fn stable(version: &str) -> GodotRelease {
        GodotRelease {
            version: version.parse().unwrap(),
            flavor: "stable".into(),
            mono: false,
        }
    }

    fn stable_mono(version: &str) -> GodotRelease {
        GodotRelease {
            version: version.parse().unwrap(),
            flavor: "stable".into(),
            mono: true,
        }
    }

    fn prerelease(version: &str, flavor: &str) -> GodotRelease {
        GodotRelease {
            version: version.parse().unwrap(),
            flavor: flavor.into(),
            mono: false,
        }
    }

    // --- URL builder --------------------------------------------------------

    #[test]
    fn template_url_standard() {
        let url = template_url(&stable("4.3"));
        assert!(url.contains("version=4.3"), "url: {url}");
        assert!(url.contains("flavor=stable"), "url: {url}");
        assert!(url.contains("slug=export_templates.tpz"), "url: {url}");
        assert!(url.contains("platform=templates"), "url: {url}");
        assert!(!url.contains("mono"), "url: {url}");
    }

    #[test]
    fn template_url_mono() {
        let url = template_url(&stable_mono("4.3"));
        assert!(url.contains("slug=mono_export_templates.tpz"), "url: {url}");
    }

    #[test]
    fn template_url_omits_zero_patch() {
        let url = template_url(&stable("4.3"));
        assert!(url.contains("version=4.3"), "url: {url}");
        assert!(!url.contains("4.3.0"), "url should not contain 4.3.0: {url}");
    }

    #[test]
    fn template_url_keeps_nonzero_patch() {
        let url = template_url(&stable("4.3.1"));
        assert!(url.contains("version=4.3.1"), "url: {url}");
    }

    #[test]
    fn template_url_prerelease() {
        let url = template_url(&prerelease("4.7", "rc1"));
        assert!(url.contains("flavor=rc1"), "url: {url}");
    }

    // --- godot_install_version ----------------------------------------------

    #[test]
    fn install_version_stable() {
        assert_eq!(godot_install_version(&stable("4.3")), "4.3.stable");
    }

    #[test]
    fn install_version_mono() {
        assert_eq!(godot_install_version(&stable_mono("4.3")), "4.3.stable.mono");
    }

    #[test]
    fn install_version_patch() {
        assert_eq!(godot_install_version(&stable("4.6.3")), "4.6.3.stable");
    }

    #[test]
    fn install_version_prerelease() {
        assert_eq!(godot_install_version(&prerelease("4.7", "rc1")), "4.7.rc1");
    }

    // --- godot_templates_subdir ---------------------------------------------

    #[test]
    fn templates_subdir_godot4() {
        assert_eq!(godot_templates_subdir(&stable("4.3")), "export_templates");
    }

    #[test]
    fn templates_subdir_godot3() {
        assert_eq!(godot_templates_subdir(&stable("3.5")), "templates");
    }

    // --- ExportTemplateCache ------------------------------------------------

    fn make_cache() -> (tempfile::TempDir, ExportTemplateCache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = ExportTemplateCache::new(dir.path().to_path_buf());
        (dir, cache)
    }

    #[test]
    fn cache_contains_returns_false_when_missing() {
        let (_dir, cache) = make_cache();
        assert!(!cache.contains(&stable("4.3")));
    }

    #[test]
    fn cache_contains_returns_false_when_version_txt_absent() {
        let (_dir, cache) = make_cache();
        let release = stable("4.3");
        std::fs::create_dir_all(cache.dir(&release).join("templates")).unwrap();
        assert!(!cache.contains(&release));
    }

    #[test]
    fn cache_contains_returns_true_when_version_txt_present() {
        let (_dir, cache) = make_cache();
        let release = stable("4.3");
        let templates_dir = cache.dir(&release).join("templates");
        std::fs::create_dir_all(&templates_dir).unwrap();
        std::fs::write(templates_dir.join("version.txt"), b"4.3.stable").unwrap();
        assert!(cache.contains(&release));
    }

    #[test]
    fn cache_remove_is_idempotent() {
        let (_dir, cache) = make_cache();
        assert!(cache.remove(&stable("4.3")).is_ok());
    }

    #[test]
    fn cache_dirs_are_separate_for_standard_and_mono() {
        let (_dir, cache) = make_cache();
        assert_ne!(cache.dir(&stable("4.3")), cache.dir(&stable_mono("4.3")));
    }

    #[test]
    fn cache_install_extracts_zip() {
        use std::io::Write;

        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buf);
            writer
                .start_file::<_, ()>("templates/version.txt", Default::default())
                .unwrap();
            writer.write_all(b"4.3.stable").unwrap();
            writer.finish().unwrap();
        }

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), buf.into_inner()).unwrap();

        let (_dir, cache) = make_cache();
        let release = stable("4.3");
        cache.install(&release, tmp.path()).unwrap();

        assert!(cache.contains(&release));
    }

    #[test]
    fn cache_install_rejects_path_traversal() {
        use std::io::Write;

        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buf);
            writer
                .start_file::<_, ()>("../../evil.txt", Default::default())
                .unwrap();
            writer.write_all(b"pwned").unwrap();
            writer.finish().unwrap();
        }

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), buf.into_inner()).unwrap();

        let (_dir, cache) = make_cache();
        assert!(cache.install(&stable("4.3"), tmp.path()).is_err());
    }

    // --- godot_install_version edge cases -----------------------------------

    #[test]
    fn install_version_zero_patch_omitted() {
        let r = GodotRelease {
            version: GodotVersion::new(4, 3, 0),
            flavor: "stable".into(),
            mono: false,
        };
        assert_eq!(godot_install_version(&r), "4.3.stable");
    }

    // --- copy_dir_all -------------------------------------------------------

    #[test]
    fn copy_dir_all_copies_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        std::fs::write(src.join("sub").join("b.txt"), b"world").unwrap();

        let dst = dir.path().join("dst");
        copy_dir_all(&src, &dst).unwrap();

        assert_eq!(std::fs::read(dst.join("a.txt")).unwrap(), b"hello");
        assert_eq!(std::fs::read(dst.join("sub").join("b.txt")).unwrap(), b"world");
    }

    #[test]
    fn copy_dir_all_destination_files_are_writable() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        // Write a read-only source file (as the cache would have).
        let src_file = src.join("file.txt");
        std::fs::write(&src_file, b"content").unwrap();
        let mut perms = std::fs::metadata(&src_file).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&src_file, perms).unwrap();

        let dst = dir.path().join("dst");
        copy_dir_all(&src, &dst).unwrap();

        let dst_perms = std::fs::metadata(dst.join("file.txt")).unwrap().permissions();
        assert!(!dst_perms.readonly(), "copied file should be writable");
    }
}
