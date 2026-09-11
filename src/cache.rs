//! Shared cache root resolution for all on-disk caches.
//!
//! All caches (Godot binaries, dependency snapshots) live under a common root:
//!
//! - `GGG_CACHE_DIR` environment variable if set
//! - Platform default otherwise:
//!   - Linux:   `~/.local/share/ggg/`
//!   - macOS:   `~/Library/Application Support/ggg/`
//!   - Windows: `%APPDATA%\ggg\`
//!
//! Each sub-cache appends its own subdirectory to this root.

use std::path::PathBuf;

use anyhow::{Context, Result};

/// Resolve the root directory for all ggg caches.
///
/// Checks `GGG_CACHE_DIR` ([`crate::envvars::CACHE_DIR_ENV_VAR`]) first, then
/// falls back to the platform default.
pub fn resolve_cache_root() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var(crate::envvars::CACHE_DIR_ENV_VAR) {
        return Ok(PathBuf::from(dir));
    }
    let data_dir = dirs::data_dir().context("could not determine the platform data directory")?;
    Ok(data_dir.join("ggg"))
}
