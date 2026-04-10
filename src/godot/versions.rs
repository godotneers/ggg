//! Fetching and parsing the Godot versions manifest.
//!
//! The manifest is a YAML file maintained by the Godot website team:
//! `https://raw.githubusercontent.com/godotengine/godot-website/master/_data/versions.yml`
//!
//! It lists every Godot release series newest-first. Each entry describes the
//! latest release for that series (stable or a pre-release flavor) plus a
//! `releases` array of older pre-release builds.
//!
//! # Example manifest fragment
//!
//! ```yaml
//! - name: "4.6.2"
//!   flavor: "stable"
//!   releases:
//!     - name: "rc2"
//!     - name: "rc1"
//!
//! - name: "4.7"
//!   flavor: "dev4"
//!   releases:
//!     - name: "dev3"
//!     - name: "dev2"
//! ```

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const VERSIONS_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/godotengine/godot-website/master/_data/versions.yml";

// --- raw manifest types ----------------------------------------------------
// These mirror the YAML structure directly. Fields we don't need are omitted;
// serde_yml ignores unknown fields by default.

#[derive(Debug, Deserialize)]
struct VersionEntry {
    name: String,
    flavor: String,
    #[serde(default)]
    releases: Vec<ReleaseEntry>,
}

#[derive(Debug, Deserialize)]
struct ReleaseEntry {
    name: String,
}

// --- public types ----------------------------------------------------------

/// A specific, downloadable Godot release - a version series combined with a
/// flavor (release stage) and a choice of standard vs. Mono (C#) build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GodotRelease {
    /// Version series, e.g. `"4.3"` or `"4.3.1"`.
    pub version: String,
    /// Release flavor, e.g. `"stable"`, `"rc1"`, `"beta2"`, `"dev4"`.
    pub flavor: String,
    /// Whether this is a Mono (C#) build.
    ///
    /// Mono builds include a `GodotSharp/` directory alongside the executable
    /// and must always be stored and invoked as a unit.
    pub mono: bool,
}

impl GodotRelease {
    /// The GitHub release tag for this release, e.g. `"4.3-stable"`.
    ///
    /// The mono flag does not affect the tag - both standard and Mono builds
    /// are published under the same release tag. The distinction appears only
    /// in the asset filename.
    pub fn tag(&self) -> String {
        format!("{}-{}", self.version, self.flavor)
    }

    /// A unique, filesystem-safe identifier for this release, used as the
    /// cache subdirectory name.
    ///
    /// Examples: `"4.3-stable"`, `"4.3-stable-mono"`, `"4.7-dev4-mono"`.
    pub fn cache_key(&self) -> String {
        if self.mono {
            format!("{}-{}-mono", self.version, self.flavor)
        } else {
            self.tag()
        }
    }

    /// Whether this is a stable release.
    pub fn is_stable(&self) -> bool {
        self.flavor == "stable"
    }

    /// Check that version and flavor contain only characters that are safe to
    /// use in a filesystem path component.
    ///
    /// This prevents path traversal attacks when the release is used to
    /// construct cache directory names. A valid version looks like `"4.3"` or
    /// `"4.3.1"` and a valid flavor looks like `"stable"`, `"rc1"`, `"dev4"`.
    pub fn validate(&self) -> Result<()> {
        validate_path_component("version", &self.version)?;
        validate_path_component("flavor", &self.flavor)?;
        Ok(())
    }
}

/// Returns an error if `value` contains characters that could escape a
/// directory when used as a path component.
///
/// Allowed: alphanumeric characters, `.`, `-`. Everything else is rejected,
/// including `/`, `\`, and `..` sequences.
fn validate_path_component(field: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("GodotRelease {field} must not be empty");
    }
    if !value.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-') {
        bail!(
            "GodotRelease {field} contains invalid characters: \"{value}\""
        );
    }
    Ok(())
}

// --- public functions -------------------------------------------------------

/// Download and parse the Godot versions manifest, returning all known
/// releases ordered newest-first.
///
/// Each version series contributes one entry per flavor: the current latest
/// (from the top-level `flavor` field) plus all older pre-releases listed in
/// `releases`.
pub fn fetch_versions() -> Result<Vec<GodotRelease>> {
    let yaml = reqwest::blocking::get(VERSIONS_MANIFEST_URL)
        .context("failed to fetch Godot versions manifest")?
        .text()
        .context("failed to read Godot versions manifest response")?;
    parse_versions(&yaml)
}

/// Parse the Godot versions manifest YAML into a list of [`GodotRelease`]s.
///
/// Separated from [`fetch_versions`] so it can be tested without network
/// access.
pub fn parse_versions(yaml: &str) -> Result<Vec<GodotRelease>> {
    let entries: Vec<VersionEntry> =
        serde_yml::from_str(yaml).context("failed to parse Godot versions manifest")?;

    let mut releases = Vec::new();
    for entry in entries {
        // The top-level flavor is the latest release for this version series.
        releases.push(GodotRelease {
            version: entry.name.clone(),
            flavor: entry.flavor,
            mono: false,
        });
        // The releases array contains older pre-release builds.
        for release in entry.releases {
            releases.push(GodotRelease {
                version: entry.name.clone(),
                flavor: release.name,
                mono: false,
            });
        }
    }

    Ok(releases)
}

// --- tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MANIFEST: &str = r#"
- name: "4.7"
  flavor: "dev4"
  releases:
    - name: "dev3"
    - name: "dev2"
    - name: "dev1"

- name: "4.6.2"
  flavor: "stable"
  releases:
    - name: "rc2"
    - name: "rc1"

- name: "4.3.1"
  flavor: "stable"
  releases:
    - name: "rc1"

- name: "4.3"
  flavor: "stable"
"#;

    #[test]
    fn parse_produces_release_for_each_flavor() {
        let releases = parse_versions(SAMPLE_MANIFEST).unwrap();
        // 4.7: dev4 + dev3 + dev2 + dev1 = 4
        // 4.6.2: stable + rc2 + rc1 = 3
        // 4.3.1: stable + rc1 = 2
        // 4.3: stable = 1
        assert_eq!(releases.len(), 10);
    }

    #[test]
    fn parse_preserves_newest_first_order() {
        let releases = parse_versions(SAMPLE_MANIFEST).unwrap();
        assert_eq!(releases[0].version, "4.7");
        assert_eq!(releases[0].flavor,  "dev4");
        assert_eq!(releases[1].flavor,  "dev3");
    }

    #[test]
    fn parse_version_with_no_prereleases() {
        let releases = parse_versions(SAMPLE_MANIFEST).unwrap();
        // 4.3 has no releases array - it should still produce one entry
        let entry_4_3: Vec<_> = releases.iter().filter(|r| r.version == "4.3").collect();
        assert_eq!(entry_4_3.len(), 1);
        assert!(entry_4_3[0].is_stable());
    }

    #[test]
    fn tag_format_is_correct() {
        let releases = parse_versions(SAMPLE_MANIFEST).unwrap();
        let stable = releases.iter().find(|r| r.version == "4.6.2" && r.is_stable()).unwrap();
        assert_eq!(stable.tag(), "4.6.2-stable");

        let dev = releases.iter().find(|r| r.version == "4.7" && r.flavor == "dev4").unwrap();
        assert_eq!(dev.tag(), "4.7-dev4");
    }

    #[test]
    fn cache_key_excludes_mono_suffix_for_standard_build() {
        let r = GodotRelease { version: "4.3".into(), flavor: "stable".into(), mono: false };
        assert_eq!(r.cache_key(), "4.3-stable");
    }

    #[test]
    fn cache_key_includes_mono_suffix_for_mono_build() {
        let r = GodotRelease { version: "4.3".into(), flavor: "stable".into(), mono: true };
        assert_eq!(r.cache_key(), "4.3-stable-mono");
    }

    #[test]
    fn tag_is_same_regardless_of_mono() {
        let standard = GodotRelease { version: "4.3".into(), flavor: "stable".into(), mono: false };
        let mono     = GodotRelease { version: "4.3".into(), flavor: "stable".into(), mono: true };
        assert_eq!(standard.tag(), mono.tag());
    }

    #[test]
    fn validate_accepts_normal_release() {
        let r = GodotRelease { version: "4.3.1".into(), flavor: "stable".into(), mono: false };
        assert!(r.validate().is_ok());
    }

    #[test]
    fn validate_rejects_path_traversal_in_version() {
        let r = GodotRelease { version: "../../etc".into(), flavor: "stable".into(), mono: false };
        assert!(r.validate().unwrap_err().to_string().contains("version"));
    }

    #[test]
    fn validate_rejects_path_traversal_in_flavor() {
        let r = GodotRelease { version: "4.3".into(), flavor: "../bad".into(), mono: false };
        assert!(r.validate().unwrap_err().to_string().contains("flavor"));
    }

    #[test]
    fn validate_rejects_empty_version() {
        let r = GodotRelease { version: "".into(), flavor: "stable".into(), mono: false };
        assert!(r.validate().unwrap_err().to_string().contains("version"));
    }

    #[test]
    fn is_stable_only_matches_stable_flavor() {
        let releases = parse_versions(SAMPLE_MANIFEST).unwrap();
        let stable_count = releases.iter().filter(|r| r.is_stable()).count();
        // 4.6.2, 4.3.1, 4.3 are stable; 4.7 is not
        assert_eq!(stable_count, 3);
    }

    #[test]
    fn parse_invalid_yaml_returns_error() {
        let result = parse_versions("this: is: not: valid: yaml: [");
        assert!(result.is_err());
    }
}
