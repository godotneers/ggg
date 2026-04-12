//! Core types for identifying a specific Godot build.
//!
//! [`GodotVersion`] represents a parsed version number (`4.3`, `4.3.1`).
//! [`GodotRelease`] is the full specification needed to fetch or cache a
//! binary: version + flavor (release stage) + mono flag.
//!
//! Both types parse from and display as canonical strings:
//!
//! ```text
//! "4.3"             -> GodotVersion { major: 4, minor: 3, patch: 0 }
//! "4.3.1"           -> GodotVersion { major: 4, minor: 3, patch: 1 }
//! "4.3-stable"      -> GodotRelease { version: 4.3, flavor: "stable", mono: false }
//! "4.3-stable-mono" -> GodotRelease { version: 4.3, flavor: "stable", mono: true }
//! ```

use std::fmt;
use std::str::FromStr;

use anyhow::{bail, Context};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ---------------------------------------------------------------------------
// GodotVersion
// ---------------------------------------------------------------------------

/// A parsed Godot version number.
///
/// Versions are compared component-wise: major, then minor, then patch.
/// The patch component is `0` when not specified, so `"4.3"` and `"4.3.0"`
/// parse to the same value and compare as equal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GodotVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl GodotVersion {
    /// Construct a `GodotVersion` directly from its components.
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }
}

impl fmt::Display for GodotVersion {
    /// Format the version, omitting the patch component when it is zero.
    ///
    /// This matches the canonical form used in Godot's version manifest,
    /// e.g. `"4.3"` rather than `"4.3.0"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.patch == 0 {
            write!(f, "{}.{}", self.major, self.minor)
        } else {
            write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

impl FromStr for GodotVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        let parse_component = |part: &str, label: &str| -> anyhow::Result<u32> {
            part.parse::<u32>()
                .map_err(|_| anyhow::anyhow!("invalid {label} component in Godot version {:?}", s))
        };
        match parts.as_slice() {
            [major, minor] => Ok(Self {
                major: parse_component(major, "major")?,
                minor: parse_component(minor, "minor")?,
                patch: 0,
            }),
            [major, minor, patch] => Ok(Self {
                major: parse_component(major, "major")?,
                minor: parse_component(minor, "minor")?,
                patch: parse_component(patch, "patch")?,
            }),
            _ => bail!(
                "invalid Godot version {:?}: expected MAJOR.MINOR or MAJOR.MINOR.PATCH",
                s
            ),
        }
    }
}

impl Serialize for GodotVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GodotVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// GodotRelease
// ---------------------------------------------------------------------------

/// A specific, downloadable Godot build - the full combination of version,
/// flavor (release stage), and standard vs. Mono (C#).
///
/// Serializes to and parses from a compact string: `"4.3-stable"` or
/// `"4.3-stable-mono"`. This is also the format used in `ggg.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GodotRelease {
    /// Version series, e.g. `4.3` or `4.3.1`.
    pub version: GodotVersion,
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

    /// Check that the flavor contains only characters that are safe to use in
    /// a filesystem path component.
    ///
    /// This prevents path traversal attacks when the release is used to
    /// construct cache directory names. The version is already guaranteed safe
    /// by the [`GodotVersion`] type. A valid flavor looks like `"stable"`,
    /// `"rc1"`, `"dev4"`.
    pub fn validate(&self) -> anyhow::Result<()> {
        validate_path_component("flavor", &self.flavor)?;
        Ok(())
    }
}

impl fmt::Display for GodotRelease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cache_key())
    }
}

impl FromStr for GodotRelease {
    type Err = anyhow::Error;

    /// Parse a release string of the form `"VERSION-FLAVOR"` or
    /// `"VERSION-FLAVOR-mono"`, e.g. `"4.3-stable"` or `"4.3-stable-mono"`.
    fn from_str(s: &str) -> anyhow::Result<Self> {
        let (base, mono) = match s.strip_suffix("-mono") {
            Some(b) => (b, true),
            None    => (s, false),
        };
        let (version_str, flavor) = base
            .split_once('-')
            .with_context(|| {
                format!("invalid Godot release {:?}: expected VERSION-FLAVOR", s)
            })?;
        let version = version_str
            .parse()
            .with_context(|| format!("invalid version in Godot release {:?}", s))?;
        Ok(Self { version, flavor: flavor.to_string(), mono })
    }
}

impl Serialize for GodotRelease {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GodotRelease {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns an error if `value` contains characters that could escape a
/// directory when used as a path component.
///
/// Allowed: alphanumeric characters, `.`, `-`. Everything else is rejected,
/// including `/`, `\`, and `..` sequences.
pub(super) fn validate_path_component(field: &str, value: &str) -> anyhow::Result<()> {
    if value.is_empty() {
        bail!("GodotRelease {field} must not be empty");
    }
    if !value.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-') {
        bail!("GodotRelease {field} contains invalid characters: \"{value}\"");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- GodotVersion -------------------------------------------------------

    #[test]
    fn version_parse_major_minor() {
        let v: GodotVersion = "4.3".parse().unwrap();
        assert_eq!(v, GodotVersion::new(4, 3, 0));
    }

    #[test]
    fn version_parse_major_minor_patch() {
        let v: GodotVersion = "4.3.1".parse().unwrap();
        assert_eq!(v, GodotVersion::new(4, 3, 1));
    }

    #[test]
    fn version_zero_patch_equals_no_patch() {
        assert_eq!("4.3.0".parse::<GodotVersion>().unwrap(), "4.3".parse::<GodotVersion>().unwrap());
    }

    #[test]
    fn version_display_omits_zero_patch() {
        assert_eq!(GodotVersion::new(4, 3, 0).to_string(), "4.3");
        assert_eq!("4.3.0".parse::<GodotVersion>().unwrap().to_string(), "4.3");
    }

    #[test]
    fn version_display_keeps_nonzero_patch() {
        assert_eq!(GodotVersion::new(4, 3, 1).to_string(), "4.3.1");
    }

    #[test]
    fn version_ordering_by_component() {
        assert!(GodotVersion::new(4, 3, 0) < GodotVersion::new(4, 3, 1));
        assert!(GodotVersion::new(4, 3, 0) < GodotVersion::new(4, 4, 0));
        assert!(GodotVersion::new(3, 5, 0) < GodotVersion::new(4, 0, 0));
    }

    #[test]
    fn version_parse_rejects_single_component() {
        assert!("4".parse::<GodotVersion>().is_err());
    }

    #[test]
    fn version_parse_rejects_four_components() {
        assert!("4.3.0.1".parse::<GodotVersion>().is_err());
    }

    #[test]
    fn version_parse_rejects_non_numeric() {
        assert!("4.x".parse::<GodotVersion>().is_err());
        assert!("4.3.beta".parse::<GodotVersion>().is_err());
    }

    #[test]
    fn version_parse_rejects_empty_string() {
        assert!("".parse::<GodotVersion>().is_err());
    }

    // --- GodotRelease -------------------------------------------------------

    #[test]
    fn release_parse_standard() {
        let r: GodotRelease = "4.3-stable".parse().unwrap();
        assert_eq!(r.version, GodotVersion::new(4, 3, 0));
        assert_eq!(r.flavor, "stable");
        assert!(!r.mono);
    }

    #[test]
    fn release_parse_mono() {
        let r: GodotRelease = "4.3-stable-mono".parse().unwrap();
        assert_eq!(r.version, GodotVersion::new(4, 3, 0));
        assert_eq!(r.flavor, "stable");
        assert!(r.mono);
    }

    #[test]
    fn release_parse_prerelease() {
        let r: GodotRelease = "4.7-dev4".parse().unwrap();
        assert_eq!(r.version, GodotVersion::new(4, 7, 0));
        assert_eq!(r.flavor, "dev4");
        assert!(!r.mono);
    }

    #[test]
    fn release_parse_prerelease_mono() {
        let r: GodotRelease = "4.3.1-rc1-mono".parse().unwrap();
        assert_eq!(r.version, GodotVersion::new(4, 3, 1));
        assert_eq!(r.flavor, "rc1");
        assert!(r.mono);
    }

    #[test]
    fn release_display_round_trips() {
        for s in &["4.3-stable", "4.3-stable-mono", "4.3.1-rc1", "4.7-dev4-mono"] {
            assert_eq!(s.parse::<GodotRelease>().unwrap().to_string(), *s);
        }
    }

    #[test]
    fn release_parse_rejects_no_flavor() {
        assert!("4.3".parse::<GodotRelease>().is_err());
    }

    #[test]
    fn release_parse_normalizes_version() {
        // "4.3.0-stable" and "4.3-stable" are the same release.
        let a: GodotRelease = "4.3.0-stable".parse().unwrap();
        let b: GodotRelease = "4.3-stable".parse().unwrap();
        assert_eq!(a, b);
        assert_eq!(a.to_string(), "4.3-stable");
    }

    #[test]
    fn release_tag_excludes_mono() {
        let r: GodotRelease = "4.3-stable-mono".parse().unwrap();
        assert_eq!(r.tag(), "4.3-stable");
    }

    #[test]
    fn release_cache_key_includes_mono() {
        let r: GodotRelease = "4.3-stable-mono".parse().unwrap();
        assert_eq!(r.cache_key(), "4.3-stable-mono");
    }

    #[test]
    fn release_validate_accepts_normal() {
        let r: GodotRelease = "4.3.1-stable".parse().unwrap();
        assert!(r.validate().is_ok());
    }

    #[test]
    fn release_validate_rejects_path_traversal_in_flavor() {
        let r = GodotRelease {
            version: GodotVersion::new(4, 3, 0),
            flavor: "../bad".into(),
            mono: false,
        };
        assert!(r.validate().unwrap_err().to_string().contains("flavor"));
    }
}
