//! Downloading Godot release archives from GitHub.
//!
//! The download process has two steps:
//!
//! 1. Query the GitHub releases API for `godotengine/godot-builds` to get the
//!    list of assets for the requested release tag, then pick the right asset
//!    for the current platform.
//! 2. Stream the asset to a temporary file, showing a progress bar, then
//!    return the path to the downloaded archive.
//!
//! The caller (typically [`super::cache::GodotCache::install`]) is responsible
//! for extracting the archive and placing it in the cache.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;

use super::release::GodotRelease;

const GODOT_BUILDS_API: &str =
    "https://api.github.com/repos/godotengine/godot-builds/releases/tags";

// --- GitHub API types ------------------------------------------------------

#[derive(Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

// --- platform detection ----------------------------------------------------

/// The current platform, used to select the right asset from a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    LinuxX86_64,
    MacOs,
    WindowsX86_64,
}

impl Platform {
    /// Detect the platform from the current compilation target.
    pub fn current() -> Result<Self> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        return Ok(Self::LinuxX86_64);

        #[cfg(target_os = "macos")]
        return Ok(Self::MacOs);

        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        return Ok(Self::WindowsX86_64);

        #[cfg(not(any(
            all(target_os = "linux", target_arch = "x86_64"),
            target_os = "macos",
            all(target_os = "windows", target_arch = "x86_64"),
        )))]
        bail!("unsupported platform - ggg supports Linux x86_64, macOS, and Windows x86_64")
    }

    /// Asset name suffixes to try for this platform, in preference order.
    ///
    /// Multiple suffixes handle naming changes across Godot versions, e.g.
    /// Linux was `_x11.64` in older releases and `_linux.x86_64` in newer
    /// ones.
    fn asset_suffixes(self) -> &'static [&'static str] {
        match self {
            Self::LinuxX86_64   => &["_linux.x86_64.zip", "_x11.64.zip", "_linux.64.zip"],
            Self::MacOs         => &["_macos.universal.zip", "_osx.universal.zip", "_osx.fat.zip"],
            // Standard builds: Godot_v4.x-stable_win64.exe.zip
            // Mono builds:     Godot_v4.x-stable_mono_win64.zip  (no .exe)
            Self::WindowsX86_64 => &["_win64.exe.zip", "_win64.zip"],
        }
    }
}

// --- asset selection -------------------------------------------------------

/// Pick the right asset from a release's asset list for the given platform
/// and mono preference.
///
/// Returns the `browser_download_url` of the selected asset.
fn select_asset(assets: &[Asset], release: &GodotRelease, platform: Platform) -> Result<String> {
    // Build the expected filename prefix. Mono assets have `_mono_` in their
    // name; standard assets must not.
    let prefix = if release.mono {
        format!("Godot_v{}-{}_mono_", release.version, release.flavor)
    } else {
        format!("Godot_v{}-{}_", release.version, release.flavor)
    };

    for suffix in platform.asset_suffixes() {
        let expected = format!("{prefix}{}", suffix.trim_start_matches('_'));
        if let Some(asset) = assets.iter().find(|a| a.name == expected) {
            return Ok(asset.browser_download_url.clone());
        }
    }

    bail!(
        "no suitable asset found for {} {} (mono: {}) on {:?}",
        release.version, release.flavor, release.mono, platform
    )
}

// --- public API ------------------------------------------------------------

/// Download the archive for `release` on the current platform to a temporary
/// file, showing a progress bar.
///
/// Returns the path to the downloaded archive. The caller is responsible for
/// cleaning up the temporary file.
pub fn download_release(release: &GodotRelease) -> Result<PathBuf> {
    release.validate()?;

    let platform = Platform::current()?;
    let url = fetch_asset_url(release, platform)?;
    download_archive(&url, release)
}

/// Query the GitHub releases API to find the download URL for the given
/// release on the given platform.
fn fetch_asset_url(release: &GodotRelease, platform: Platform) -> Result<String> {
    let url = format!("{}/{}", GODOT_BUILDS_API, release.tag());

    let response = reqwest::blocking::Client::new()
        .get(&url)
        // GitHub API requires a User-Agent header.
        .header("User-Agent", "ggg")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .with_context(|| format!("failed to query GitHub releases API for {}", release.tag()))?;

    if !response.status().is_success() {
        bail!(
            "GitHub releases API returned {} for tag {}",
            response.status(),
            release.tag()
        );
    }

    let release_data: Release = response
        .json()
        .context("failed to parse GitHub releases API response")?;

    select_asset(&release_data.assets, release, platform)
}

/// Stream a file from `url` to a temporary file, showing a progress bar.
fn download_archive(url: &str, release: &GodotRelease) -> Result<PathBuf> {
    let mut response = reqwest::blocking::Client::new()
        .get(url)
        .header("User-Agent", "ggg")
        .send()
        .with_context(|| format!("failed to start download from {url}"))?;

    if !response.status().is_success() {
        bail!("download failed with status {}", response.status());
    }

    // Use Content-Length for the progress bar if the server provides it.
    let total_bytes = response.content_length();

    let pb = ProgressBar::new(total_bytes.unwrap_or(0));
    pb.set_style(
        ProgressStyle::with_template(
            "{msg} [{bar:40}] {bytes}/{total_bytes} ({bytes_per_sec}, eta {eta})",
        )?
        .progress_chars("=> "),
    );
    pb.set_message(format!("Downloading Godot {}", release.tag()));

    // Stream into a named temporary file so partial downloads don't leave
    // a corrupt file in place if we are interrupted.
    let mut tmp = tempfile::NamedTempFile::new()
        .context("failed to create temporary file for download")?;

    let mut buf = [0u8; 8192];
    loop {
        let n = std::io::Read::read(&mut response, &mut buf)
            .context("error reading download stream")?;
        if n == 0 {
            break;
        }
        tmp.write_all(&buf[..n])
            .context("error writing to temporary file")?;
        pb.inc(n as u64);
    }

    pb.finish_with_message(format!("Downloaded Godot {}", release.tag()));

    // Persist the temp file so it survives beyond this function's scope.
    let (_, path) = tmp.keep()
        .context("failed to persist downloaded archive")?;

    Ok(path)
}

// --- tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str, flavor: &str, mono: bool) -> GodotRelease {
        GodotRelease { version: version.parse().unwrap(), flavor: flavor.into(), mono }
    }

    fn make_assets(names: &[&str]) -> Vec<Asset> {
        names.iter().map(|n| Asset {
            name: n.to_string(),
            browser_download_url: format!("https://example.com/{n}"),
        }).collect()
    }

    #[test]
    fn select_asset_picks_correct_linux_asset() {
        let assets = make_assets(&[
            "Godot_v4.3-stable_linux.x86_64.zip",
            "Godot_v4.3-stable_win64.exe.zip",
            "Godot_v4.3-stable_macos.universal.zip",
        ]);
        let url = select_asset(&assets, &release("4.3", "stable", false), Platform::LinuxX86_64).unwrap();
        assert!(url.contains("linux.x86_64"));
    }

    #[test]
    fn select_asset_picks_mono_asset_when_requested() {
        let assets = make_assets(&[
            "Godot_v4.3-stable_linux.x86_64.zip",
            "Godot_v4.3-stable_mono_linux.x86_64.zip",
        ]);
        let url = select_asset(&assets, &release("4.3", "stable", true), Platform::LinuxX86_64).unwrap();
        assert!(url.contains("mono"));
    }

    #[test]
    fn select_asset_does_not_pick_mono_for_standard_release() {
        let assets = make_assets(&[
            "Godot_v4.3-stable_linux.x86_64.zip",
            "Godot_v4.3-stable_mono_linux.x86_64.zip",
        ]);
        let url = select_asset(&assets, &release("4.3", "stable", false), Platform::LinuxX86_64).unwrap();
        assert!(!url.contains("mono"));
    }

    #[test]
    fn select_asset_falls_back_to_legacy_suffix() {
        // Older Godot releases used _x11.64 instead of _linux.x86_64.
        let assets = make_assets(&["Godot_v3.5-stable_x11.64.zip"]);
        let url = select_asset(&assets, &release("3.5", "stable", false), Platform::LinuxX86_64).unwrap();
        assert!(url.contains("x11.64"));
    }

    #[test]
    fn select_asset_picks_mono_windows_asset() {
        // Mono Windows builds use _win64.zip, not _win64.exe.zip.
        let assets = make_assets(&[
            "Godot_v4.6-stable_win64.exe.zip",
            "Godot_v4.6-stable_mono_win64.zip",
        ]);
        let url = select_asset(&assets, &release("4.6", "stable", true), Platform::WindowsX86_64).unwrap();
        assert!(url.contains("mono"));
        assert!(url.contains("win64.zip"));
    }

    #[test]
    fn select_asset_picks_standard_windows_asset() {
        let assets = make_assets(&[
            "Godot_v4.6-stable_win64.exe.zip",
            "Godot_v4.6-stable_mono_win64.zip",
        ]);
        let url = select_asset(&assets, &release("4.6", "stable", false), Platform::WindowsX86_64).unwrap();
        assert!(!url.contains("mono"));
        assert!(url.contains("win64.exe.zip"));
    }

    #[test]
    fn select_asset_returns_error_when_no_match() {
        let assets = make_assets(&["Godot_v4.3-stable_win64.exe.zip"]);
        let result = select_asset(&assets, &release("4.3", "stable", false), Platform::LinuxX86_64);
        assert!(result.unwrap_err().to_string().contains("no suitable asset"));
    }

}
