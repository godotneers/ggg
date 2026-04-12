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

use anyhow::{Context, Result};
use serde::Deserialize;

use super::release::GodotRelease;

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
        let version = match entry.name.parse::<super::release::GodotVersion>() {
            Ok(v) => v,
            Err(_) => continue, // skip entries with unrecognised version formats
        };

        // The top-level flavor is the latest release for this version series.
        releases.push(GodotRelease { version: version.clone(), flavor: entry.flavor, mono: false });

        // The releases array contains older pre-release builds for this series.
        for release in entry.releases {
            releases.push(GodotRelease { version: version.clone(), flavor: release.name, mono: false });
        }
    }

    Ok(releases)
}

// --- tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::godot::release::GodotVersion;

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
        assert_eq!(releases[0].version, GodotVersion::new(4, 7, 0));
        assert_eq!(releases[0].flavor,  "dev4");
        assert_eq!(releases[1].flavor,  "dev3");
    }

    #[test]
    fn parse_version_with_no_prereleases() {
        let releases = parse_versions(SAMPLE_MANIFEST).unwrap();
        let entry_4_3: Vec<_> = releases
            .iter()
            .filter(|r| r.version == GodotVersion::new(4, 3, 0))
            .collect();
        // 4.3 has no releases array - it should still produce one entry
        assert_eq!(entry_4_3.len(), 1);
        assert!(entry_4_3[0].is_stable());
    }

    #[test]
    fn parse_invalid_yaml_returns_error() {
        let result = parse_versions("this: is: not: valid: yaml: [");
        assert!(result.is_err());
    }

    #[test]
    fn parse_skips_unrecognised_version_formats() {
        let yaml = r#"
- name: "2.0.4.1"
  flavor: "stable"

- name: "4.3"
  flavor: "stable"
"#;
        let releases = parse_versions(yaml).unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].version, GodotVersion::new(4, 3, 0));
    }

    #[test]
    fn is_stable_only_matches_stable_flavor() {
        let releases = parse_versions(SAMPLE_MANIFEST).unwrap();
        let stable_count = releases.iter().filter(|r| r.is_stable()).count();
        // 4.6.2, 4.3.1, 4.3 are stable; 4.7 is not
        assert_eq!(stable_count, 3);
    }
}
