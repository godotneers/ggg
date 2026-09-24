//! High-level operations on Godot engine binaries.
//!
//! This module provides two primitives that commands compose together:
//!
//! - [`ensure`] - guarantee a release is present in the cache, downloading it
//!   if necessary. Returns the path to the executable.
//! - [`resolve`] - like [`ensure`], but honours a user-supplied executable
//!   override (`--godot` flag or `GGG_GODOT_EXECUTABLE`) first. When an
//!   override is given, ggg does not manage the binary at all.
//! - [`launch`] - run a Godot executable with the given arguments, forwarding
//!   stdin, stdout, and stderr to the terminal. Returns the process exit status.
//!
//! `ensure` and `launch` are intentionally separate: `ggg sync` needs `ensure`
//! but not `launch`; `ggg edit` and `ggg run` use both in sequence. `resolve`
//! is the entry point commands call so the override precedence lives in one
//! place.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{Context, Result, bail};

use super::cache::GodotCache;
use super::download;
use super::release::GodotRelease;

// ---------------------------------------------------------------------------
// Executable override
// ---------------------------------------------------------------------------

/// Resolve which Godot executable to use, applying the override precedence:
///
/// ```text
/// --godot flag  >  GGG_GODOT_EXECUTABLE  >  managed default
/// ```
///
/// With no override, delegates to [`ensure`] so the pinned release is
/// downloaded/cached as usual. When an override is present it is validated
/// (must exist and be a regular file) and returned as-is; ggg never downloads
/// or manages the binary in that case.
pub fn resolve(
    release: &GodotRelease,
    cache: &GodotCache,
    cli_path: Option<&Path>,
) -> Result<PathBuf> {
    match cli_path.map(Path::to_path_buf).or(env_executable()) {
        Some(path) => {
            let via = if cli_path.is_some() {
                "`--godot`"
            } else {
                "`$GGG_GODOT_EXECUTABLE`"
            };
            validate_executable(&path, via)?;
            Ok(path)
        }
        None => ensure(release, cache),
    }
}

/// Ensure `release` is present in `cache`, downloading and installing it if
/// not. Returns the path to the Godot executable.
fn ensure(release: &GodotRelease, cache: &GodotCache) -> Result<PathBuf> {
    if cache.contains(release) {
        return cache.executable_path(release);
    }

    let archive = download::download_release(release)
        .with_context(|| format!("failed to download Godot {}", release.tag()))?;

    let executable = cache
        .install(release, &archive)
        .with_context(|| format!("failed to install Godot {}", release.tag()))?;

    // Clean up the downloaded archive now that it has been extracted.
    let _ = std::fs::remove_file(&archive);

    Ok(executable)
}

/// The path from `GGG_GODOT_EXECUTABLE`, or `None` when the variable is unset
/// or set to an empty string (an empty value is treated as no override).
fn env_executable() -> Option<PathBuf> {
    std::env::var(crate::envvars::GODOT_EXECUTABLE_ENV_VAR)
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Guardrail: an override must point at a real, regular file. Produces a clear
/// error naming where the path came from instead of panicking or falling back
/// to a managed download.
fn validate_executable(path: &Path, via: &str) -> Result<()> {
    if !path.exists() {
        bail!(
            "the Godot executable from {} does not exist: {}",
            via,
            path.display()
        );
    }
    if !path.is_file() {
        bail!(
            "the Godot executable from {} is not a file: {}",
            via,
            path.display()
        );
    }
    Ok(())
}

/// Launch the Godot executable at `path` with the given `args`, inheriting
/// stdin, stdout, and stderr from the current process.
///
/// The path must have been obtained from [`ensure`] - passing an arbitrary
/// path that does not point to a real executable is a programmer error.
///
/// Returns the exit status of the Godot process.
pub fn launch(executable: &Path, args: &[String]) -> Result<ExitStatus> {
    Command::new(executable)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("failed to launch Godot at {}", executable.display()))
}

// --- tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::godot::cache::GodotCache;
    use serial_test::serial;

    fn temp_file() -> tempfile::NamedTempFile {
        tempfile::NamedTempFile::new().unwrap()
    }

    // --- env parsing --------------------------------------------------------

    #[test]
    #[serial]
    fn env_executable_ignores_empty_value() {
        // An explicitly empty variable is treated as "no override", mirroring
        // how uv treats an empty UV_PYTHON rather than producing a do-nothing
        // path.
        unsafe {
            std::env::set_var(crate::envvars::GODOT_EXECUTABLE_ENV_VAR, "");
        }
        assert_eq!(env_executable(), None);
        unsafe { std::env::remove_var(crate::envvars::GODOT_EXECUTABLE_ENV_VAR) };
    }

    #[test]
    #[serial]
    fn env_executable_reads_nonempty_value() {
        unsafe {
            std::env::set_var(
                crate::envvars::GODOT_EXECUTABLE_ENV_VAR,
                "/tmp/custom-godot",
            );
        }
        assert_eq!(env_executable(), Some(PathBuf::from("/tmp/custom-godot")));
        unsafe { std::env::remove_var(crate::envvars::GODOT_EXECUTABLE_ENV_VAR) };
    }

    #[test]
    #[serial]
    fn env_executable_none_when_unset() {
        unsafe { std::env::remove_var(crate::envvars::GODOT_EXECUTABLE_ENV_VAR) };
        assert_eq!(env_executable(), None);
    }

    // --- validation --------------------------------------------------------

    #[test]
    fn validate_accepts_existing_file() {
        let file = temp_file();
        assert!(validate_executable(file.path(), "`--godot`").is_ok());
    }

    #[test]
    fn validate_rejects_missing_path_and_names_source() {
        let missing = PathBuf::from("/does/not/exist/godot");
        let err = validate_executable(&missing, "`--godot`")
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not exist"), "err: {err}");
        assert!(err.contains("`--godot`"), "err: {err}");
        assert!(err.contains("/does/not/exist/godot"), "err: {err}");
    }

    #[test]
    fn validate_rejects_directory_and_names_source() {
        let dir = tempfile::tempdir().unwrap();
        let err = validate_executable(dir.path(), "`$GGG_GODOT_EXECUTABLE`")
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a file"), "err: {err}");
        assert!(err.contains("`$GGG_GODOT_EXECUTABLE`"), "err: {err}");
    }

    // --- resolve -----------------------------------------------------------

    #[test]
    fn resolve_returns_cli_override_without_managing() {
        let file = temp_file();
        let dir = tempfile::tempdir().unwrap();
        let cache = GodotCache::new(dir.path().to_path_buf());
        let release: GodotRelease = "4.3-stable".parse().unwrap();

        // With a --godot override the release is neither cached nor downloaded:
        // the empty cache is never touched and the override path is returned.
        let resolved = resolve(&release, &cache, Some(file.path())).unwrap();
        assert_eq!(resolved, file.path());
        assert!(!cache.contains(&release));
    }

    #[test]
    fn resolve_errors_on_invalid_override() {
        let dir = tempfile::tempdir().unwrap();
        let cache = GodotCache::new(dir.path().to_path_buf());
        let release: GodotRelease = "4.3-stable".parse().unwrap();
        let missing = dir.path().join("nope");

        let err = resolve(&release, &cache, Some(&missing))
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not exist"), "err: {err}");
        assert!(err.contains("`--godot`"), "err: {err}");
        assert!(!cache.contains(&release));
    }
}
