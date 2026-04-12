//! High-level operations on Godot engine binaries.
//!
//! This module provides two primitives that commands compose together:
//!
//! - [`ensure`] - guarantee a release is present in the cache, downloading it
//!   if necessary. Returns the path to the executable.
//! - [`launch`] - run a Godot executable with the given arguments, forwarding
//!   stdin, stdout, and stderr to the terminal. Returns the process exit status.
//!
//! The two are intentionally separate: `ggg sync` needs `ensure` but not
//! `launch`; `ggg edit` and `ggg run` use both in sequence.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{Context, Result};

use super::cache::GodotCache;
use super::download;
use super::release::GodotRelease;

/// Ensure `release` is present in `cache`, downloading and installing it if
/// not. Returns the path to the Godot executable.
pub fn ensure(release: &GodotRelease, cache: &GodotCache) -> Result<PathBuf> {
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
