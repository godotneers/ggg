//! Local cache for downloaded Godot engine binaries.
//!
//! Each release is stored in its own subdirectory, named after the release's
//! cache key (e.g. `4.3-stable` or `4.3-stable-mono`). The subdirectory
//! contains the extracted archive contents - on Windows this includes both
//! the executable and, for Mono builds, the `GodotSharp/` directory.
//!
//! # Cache location
//!
//! Resolved in priority order:
//! 1. `GGG_CACHE_DIR` environment variable
//! 2. Platform default:
//!    - Linux:   `~/.local/share/ggg/godot/`
//!    - macOS:   `~/Library/Application Support/ggg/godot/`
//!    - Windows: `%APPDATA%\ggg\godot\`

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::release::GodotRelease;

use crate::cache::resolve_cache_root;

/// Manages the on-disk cache of extracted Godot engine binaries.
pub struct GodotCache {
    base: PathBuf,
}

impl GodotCache {
    /// Create a cache rooted at an explicit path.
    ///
    /// Useful in tests - pass a `tempdir` path to get an isolated cache.
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }

    /// Resolve the cache location from the environment and create a cache
    /// rooted there.
    ///
    /// Checks `GGG_CACHE_DIR` first, then falls back to the platform default.
    pub fn from_env() -> Result<Self> {
        Ok(Self::new(resolve_cache_root()?.join("godot")))
    }

    /// Returns `true` if this release is already extracted in the cache.
    pub fn contains(&self, release: &GodotRelease) -> bool {
        // We consider a release cached if its directory exists and contains
        // at least one file - an empty directory is not a valid install.
        let dir = self.release_dir(release);
        dir.is_dir() && dir.read_dir().map_or(false, |mut d| d.next().is_some())
    }

    /// Returns the path to the Godot executable for this release.
    ///
    /// The release must already be installed - use [`contains`](Self::contains)
    /// to check first, or call [`install`](Self::install) to download and
    /// extract it.
    pub fn executable_path(&self, release: &GodotRelease) -> Result<PathBuf> {
        let dir = self.release_dir(release);
        find_executable(&dir)
    }

    /// Extract a downloaded archive into the cache and return the path to
    /// the Godot executable.
    ///
    /// The archive is extracted into a subdirectory named after the release's
    /// cache key. Any previous contents of that directory are removed first so
    /// a re-install always results in a clean state.
    ///
    /// On Unix the executable bit is set on the extracted binary.
    pub fn install(&self, release: &GodotRelease, archive: &Path) -> Result<PathBuf> {
        release.validate()?;

        let dir = self.release_dir(release);

        // Remove any previous (possibly partial) install.
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .with_context(|| format!("failed to remove existing install at {}", dir.display()))?;
        }
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create cache directory {}", dir.display()))?;

        crate::utils::archive::scan_zip(archive)
            .with_context(|| format!("Godot archive contains unsafe paths - refusing to extract"))?;
        crate::utils::archive::extract_zip(archive, &dir)?;

        let executable = find_executable(&dir)?;

        #[cfg(unix)]
        set_executable_bit(&executable)?;

        Ok(executable)
    }

    /// Remove a cached release, freeing its disk space.
    ///
    /// Does nothing if the release is not cached.
    pub fn remove(&self, release: &GodotRelease) -> Result<()> {
        release.validate()?;

        let dir = self.release_dir(release);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .with_context(|| format!("failed to remove {}", dir.display()))?;
        }
        Ok(())
    }

    /// The directory where a release's extracted files are stored.
    fn release_dir(&self, release: &GodotRelease) -> PathBuf {
        self.base.join(release.cache_key())
    }
}

// --- executable selection --------------------------------------------------

/// Find the Godot executable within an extracted release directory.
///
/// On Windows, Godot ships two executables: a standard one and a console
/// variant (name contains `_console`). We always prefer the non-console one.
/// On Linux and macOS there is only one executable.
///
/// Searches up to two levels deep because the Windows mono zip wraps
/// everything in a single subdirectory (e.g. `Godot_v4.6-stable_mono_win64/`)
/// while the standard Windows zip places the exe directly at the root.
fn find_executable(dir: &Path) -> Result<PathBuf> {
    let candidates = collect_executables(dir, 2);

    match candidates.len() {
        0 => bail!("no Godot executable found in {}", dir.display()),
        1 => Ok(candidates.into_iter().next().unwrap()),
        _ => {
            // Multiple candidates: prefer the non-console one.
            candidates
                .into_iter()
                .find(|p| !is_console_executable(p))
                .context("could not find a non-console Godot executable")
        }
    }
}

/// Collect all Godot executables within `dir`, searching up to `depth` levels.
fn collect_executables(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return result,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && is_godot_executable(&path) {
            result.push(path);
        } else if path.is_dir() && depth > 1 {
            result.extend(collect_executables(&path, depth - 1));
        }
    }
    result
}

/// Returns `true` if the path looks like a Godot executable.
fn is_godot_executable(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n.to_lowercase(),
        None => return false,
    };
    // Must start with "godot" and be an executable file type.
    name.starts_with("godot") && (
        name.ends_with(".exe")     // Windows
        || name.contains("linux")  // Linux
        || name.ends_with(".app")  // macOS app bundle
        // macOS universal binary has no extension
        || name.contains("macos")
    )
}

/// Returns `true` if this is the Windows console-window variant of the
/// Godot executable, which we want to avoid launching.
fn is_console_executable(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map_or(false, |n| n.to_lowercase().contains("console"))
}

#[cfg(unix)]
fn set_executable_bit(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .with_context(|| format!("failed to read permissions of {}", path.display()))?
        .permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("failed to set executable bit on {}", path.display()))
}

// --- tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cache() -> (tempfile::TempDir, GodotCache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = GodotCache::new(dir.path().to_path_buf());
        (dir, cache)
    }

    fn stable(version: &str) -> GodotRelease {
        GodotRelease { version: version.parse().unwrap(), flavor: "stable".into(), mono: false }
    }

    fn stable_mono(version: &str) -> GodotRelease {
        GodotRelease { version: version.parse().unwrap(), flavor: "stable".into(), mono: true }
    }

    #[test]
    fn contains_returns_false_for_missing_release() {
        let (_dir, cache) = make_cache();
        assert!(!cache.contains(&stable("4.3")));
    }

    #[test]
    fn contains_returns_false_for_empty_directory() {
        let (_dir, cache) = make_cache();
        let release = stable("4.3");
        std::fs::create_dir_all(cache.release_dir(&release)).unwrap();
        assert!(!cache.contains(&release));
    }

    #[test]
    fn remove_is_idempotent_for_missing_release() {
        let (_dir, cache) = make_cache();
        assert!(cache.remove(&stable("4.3")).is_ok());
    }

    #[test]
    fn release_dirs_are_separate_for_standard_and_mono() {
        let (_dir, cache) = make_cache();
        let std_dir  = cache.release_dir(&stable("4.3"));
        let mono_dir = cache.release_dir(&stable_mono("4.3"));
        assert_ne!(std_dir, mono_dir);
    }

    #[test]
    fn from_env_uses_env_var_when_set() {
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: this is a single-threaded test binary so mutating the
        // environment here cannot race with other threads.
        unsafe { std::env::set_var(crate::cache::CACHE_DIR_ENV_VAR, dir.path()); }
        let cache = GodotCache::from_env().unwrap();
        // from_env appends "godot" to the cache root.
        assert_eq!(cache.base, dir.path().join("godot"));
        unsafe { std::env::remove_var(crate::cache::CACHE_DIR_ENV_VAR); }
    }

    #[test]
    fn remove_rejects_release_with_path_traversal_in_flavor() {
        let (_dir, cache) = make_cache();
        let bad = GodotRelease {
            version: "4.3".parse().unwrap(),
            flavor: "../bad".into(),
            mono: false,
        };
        assert!(cache.remove(&bad).unwrap_err().to_string().contains("flavor"));
    }

    #[test]
    fn install_rejects_archive_with_path_traversal() {
        use std::io::Write;

        // Build a zip with a path traversal entry.
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buf);
            writer.start_file::<_, ()>("../../evil.txt", Default::default()).unwrap();
            writer.write_all(b"pwned").unwrap();
            writer.finish().unwrap();
        }

        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("evil.zip");
        std::fs::write(&archive_path, buf.into_inner()).unwrap();

        let (_cache_dir, cache) = make_cache();
        let release = stable("4.3");
        let result = cache.install(&release, &archive_path);
        assert!(result.is_err());
    }

    #[test]
    fn find_executable_finds_exe_in_subdirectory() {
        // Mono Windows zips wrap everything in a folder, e.g.
        // Godot_v4.6-stable_mono_win64/Godot_v4.6-stable_mono_win64.exe
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("Godot_v4.6-stable_mono_win64");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("Godot_v4.6-stable_mono_win64.exe"), b"").unwrap();
        std::fs::write(sub.join("Godot_v4.6-stable_mono_win64_console.exe"), b"").unwrap();

        let exe = find_executable(dir.path()).unwrap();
        assert!(exe.to_string_lossy().contains("mono_win64.exe"));
        assert!(!exe.to_string_lossy().contains("console"));
    }

    #[test]
    fn find_executable_finds_exe_at_root() {
        // Standard Windows zips place the exe directly at the root.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Godot_v4.6-stable_win64.exe"), b"").unwrap();
        std::fs::write(dir.path().join("Godot_v4.6-stable_win64_console.exe"), b"").unwrap();

        let exe = find_executable(dir.path()).unwrap();
        assert!(exe.to_string_lossy().contains("win64.exe"));
        assert!(!exe.to_string_lossy().contains("console"));
    }

    #[test]
    fn is_console_executable_detects_console_variant() {
        let console = PathBuf::from("Godot_v4.3-stable_win64_console.exe");
        let normal  = PathBuf::from("Godot_v4.3-stable_win64.exe");
        assert!( is_console_executable(&console));
        assert!(!is_console_executable(&normal));
    }

    #[test]
    fn is_godot_executable_recognises_platform_variants() {
        assert!(is_godot_executable(&PathBuf::from("Godot_v4.3-stable_win64.exe")));
        assert!(is_godot_executable(&PathBuf::from("Godot_v4.3-stable_linux.x86_64")));
        assert!(is_godot_executable(&PathBuf::from("Godot_v4.3-stable_macos.universal")));
        assert!(!is_godot_executable(&PathBuf::from("GodotSharp")));
        assert!(!is_godot_executable(&PathBuf::from("readme.txt")));
    }
}
