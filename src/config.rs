//! Typed representation of `ggg.toml` and the operations to read/write it.
//!
//! # File format
//!
//! ```toml
//! [project]
//! godot = "4.3-stable"
//!
//! [[dependency]]
//! name = "gut"
//! git  = "https://github.com/bitwes/Gut.git"
//! rev  = "v9.3.0"
//! map  = [
//!     { from = "addons/gut" },
//!     { from = "examples/", to = "examples/gut" },
//! ]
//! ```
//!
//! # Round-trip safety
//!
//! [`Config::load`] and [`Config::save`] use `toml_edit` rather than plain
//! `toml`. `toml_edit` preserves the original formatting and comments in the
//! parts of the file that are not modified, which matters when commands like
//! `ggg add` or `ggg remove` make programmatic changes to a hand-written file.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::godot::release::GodotRelease;

/// The full contents of a `ggg.toml` file.
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    /// `[project]` - engine version declaration.
    pub project: Project,
    /// `[[dependency]]` - zero or more addon dependencies.
    ///
    /// Deserialises as an empty `Vec` when no `[[dependency]]` tables are
    /// present, so callers never need to handle a missing key explicitly.
    #[serde(default)]
    pub dependency: Vec<Dependency>,
}

/// The `[project]` table.
#[derive(Debug, Deserialize, Serialize)]
pub struct Project {
    /// The exact Godot build to use, e.g. `"4.3-stable"` or `"4.3-stable-mono"`.
    ///
    /// `ggg sync` downloads this binary if it is not already cached.
    /// `ggg edit` and `ggg run` invoke it.
    pub godot: GodotRelease,
}

/// One `[[dependency]]` entry - a single addon sourced from a git repository.
#[derive(Debug, Deserialize, Serialize)]
pub struct Dependency {
    /// Short identifier for this dependency, unique within the file.
    ///
    /// Used in CLI output, the lock file, and as the argument to `ggg remove`.
    pub name: String,

    /// HTTPS or SSH URL of the git repository.
    pub git: String,

    /// Tag, branch, or full commit SHA to check out.
    ///
    /// `ggg sync` resolves tags and branches to a commit SHA and records it
    /// in `ggg.lock` so subsequent syncs are reproducible regardless of
    /// upstream changes.
    pub rev: String,

    /// Path mappings that control which parts of the repository are installed
    /// and where. When absent the entire repository is copied into the project
    /// root as-is.
    ///
    /// See [`MapEntry`] for the per-entry semantics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<Vec<MapEntry>>,
}

/// One entry in a dependency's `map` array.
///
/// Describes a single path to copy from the repository into the project.
///
/// # Examples
///
/// Symmetric - install `addons/gut` from the repo to `addons/gut` in the
/// project (`to` is omitted because it equals `from`):
/// ```toml
/// { from = "addons/gut" }
/// ```
///
/// Renamed - install `examples/` from the repo to `examples/gut` in the
/// project:
/// ```toml
/// { from = "examples/", to = "examples/gut" }
/// ```
#[derive(Debug, Deserialize, Serialize)]
pub struct MapEntry {
    /// Path within the git repository (file or directory).
    pub from: String,

    /// Destination path inside the Godot project, relative to the project
    /// root. Defaults to [`from`](MapEntry::from) when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}


impl Config {
    /// Read and deserialise a `ggg.toml` file from `path`, then validate it.
    ///
    /// Returns an error if the file cannot be read, if the TOML is invalid,
    /// or if validation fails (e.g. duplicate dependency names).
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!(
                    "no ggg.toml found in the current directory - run `ggg init` to create one"
                )
            } else {
                anyhow::anyhow!("failed to read {}: {}", path.display(), e)
            }
        })?;
        let config: Self = toml_edit::de::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    /// Check that the config is internally consistent.
    ///
    /// Currently enforces that all dependency names are unique, since `name`
    /// is used as an identifier in CLI commands and the lock file.
    pub fn validate(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for dep in &self.dependency {
            if !seen.insert(&dep.name) {
                anyhow::bail!("duplicate dependency name: \"{}\"", dep.name);
            }
        }
        Ok(())
    }

    /// Validate, serialise, and write this config to `path`, overwriting any
    /// existing file.
    ///
    /// Validation runs before any I/O, so an invalid config is rejected
    /// without touching the file.
    ///
    /// Uses `toml_edit`'s pretty serialiser so the output is human-readable.
    /// Note that this overwrites the file completely; to make surgical edits
    /// while preserving comments, work with a `toml_edit::DocumentMut` directly
    /// instead.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let content = toml_edit::ser::to_string_pretty(self)
            .context("failed to serialize config")?;
        std::fs::write(path, content)
            .with_context(|| format!("failed to write {}", path.display()))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::godot::release::GodotRelease;

    // --- helpers -----------------------------------------------------------

    /// Parse a TOML string directly into a [`Config`], without touching the
    /// filesystem. Used by most tests so they stay fast and self-contained.
    fn parse(toml: &str) -> Config {
        toml_edit::de::from_str(toml).expect("test TOML should be valid")
    }

    /// Serialise a [`Config`] back to a TOML string.
    fn serialize(config: &Config) -> String {
        toml_edit::ser::to_string_pretty(config).expect("serialization should not fail")
    }

    // --- parsing -----------------------------------------------------------

    #[test]
    fn parse_minimal_config() {
        // A config with only [project] and no dependencies is valid.
        let config = parse(r#"
            [project]
            godot = "4.3-stable"
        "#);

        assert_eq!(config.project.godot, "4.3-stable".parse::<GodotRelease>().unwrap());
        assert!(config.dependency.is_empty());
    }

    #[test]
    fn parse_dependency_without_map() {
        let config = parse(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
        "#);

        assert_eq!(config.dependency.len(), 1);
        let dep = &config.dependency[0];
        assert_eq!(dep.name, "gut");
        assert_eq!(dep.rev,  "v9.3.0");
        assert!(dep.map.is_none());
    }

    #[test]
    fn parse_dependency_with_symmetric_map() {
        // When `to` is omitted, the entry is still valid - the field is None.
        let config = parse(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
            map  = [{ from = "addons/gut" }]
        "#);

        let map = config.dependency[0].map.as_ref().unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map[0].from, "addons/gut");
        assert!(map[0].to.is_none());
    }

    #[test]
    fn parse_dependency_with_renamed_map() {
        let config = parse(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
            map  = [
                { from = "addons/gut" },
                { from = "examples/", to = "examples/gut" },
            ]
        "#);

        let map = config.dependency[0].map.as_ref().unwrap();
        assert_eq!(map.len(), 2);
        assert!(map[0].to.is_none());
        assert_eq!(map[1].to.as_deref(), Some("examples/gut"));
    }

    #[test]
    fn parse_multiple_dependencies() {
        let config = parse(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"

            [[dependency]]
            name = "phantom-camera"
            git  = "https://github.com/ramokz/phantom-camera.git"
            rev  = "v0.8"
        "#);

        assert_eq!(config.dependency.len(), 2);
        assert_eq!(config.dependency[1].name, "phantom-camera");
    }

    // --- required field errors ---------------------------------------------

    #[test]
    fn parse_missing_godot_field_fails() {
        let result = toml_edit::de::from_str::<Config>(r#"
            [project]
        "#);
        assert!(result.unwrap_err().to_string().contains("missing field `godot`"));
    }

    #[test]
    fn parse_missing_dependency_name_fails() {
        let result = toml_edit::de::from_str::<Config>(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            git = "https://github.com/bitwes/Gut.git"
            rev = "v9.3.0"
        "#);
        assert!(result.unwrap_err().to_string().contains("missing field `name`"));
    }

    #[test]
    fn parse_missing_dependency_git_fails() {
        let result = toml_edit::de::from_str::<Config>(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            rev  = "v9.3.0"
        "#);
        assert!(result.unwrap_err().to_string().contains("missing field `git`"));
    }

    #[test]
    fn parse_missing_dependency_rev_fails() {
        let result = toml_edit::de::from_str::<Config>(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
        "#);
        assert!(result.unwrap_err().to_string().contains("missing field `rev`"));
    }

    #[test]
    fn parse_missing_map_entry_from_fails() {
        let result = toml_edit::de::from_str::<Config>(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
            map  = [{ to = "addons/gut" }]
        "#);
        assert!(result.unwrap_err().to_string().contains("missing field `from`"));
    }

    // --- validation --------------------------------------------------------

    #[test]
    fn validate_rejects_duplicate_dependency_names() {
        let config = parse(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.1"
        "#);

        assert!(config.validate().unwrap_err().to_string().contains("gut"));
    }

    #[test]
    fn validate_accepts_unique_dependency_names() {
        let config = parse(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"

            [[dependency]]
            name = "phantom-camera"
            git  = "https://github.com/ramokz/phantom-camera.git"
            rev  = "v0.8"
        "#);

        assert!(config.validate().is_ok());
    }

    // --- serialization -----------------------------------------------------

    #[test]
    fn absent_map_is_not_serialized() {
        // Optional fields set to None must not appear in the output at all,
        // so the written file stays clean and round-trips back correctly.
        let config = parse(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
        "#);

        let output = serialize(&config);
        assert!(!output.contains("map"));
    }

    #[test]
    fn absent_map_entry_to_is_not_serialized() {
        // Same principle for the `to` field inside a map entry.
        let config = parse(r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
            map  = [{ from = "addons/gut" }]
        "#);

        let output = serialize(&config);
        assert!(!output.contains("to ="));
    }

    // --- file I/O ----------------------------------------------------------

    #[test]
    fn load_and_save_round_trip() {
        // Write a config to a temp file, load it back, and verify the values
        // survived the round trip.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggg.toml");

        let original = Config {
            project: Project { godot: "4.3-stable".parse().unwrap() },
            dependency: vec![
                Dependency {
                    name: "gut".into(),
                    git:  "https://github.com/bitwes/Gut.git".into(),
                    rev:  "v9.3.0".into(),
                    map:  Some(vec![
                        MapEntry { from: "addons/gut".into(), to: None },
                    ]),
                },
            ],
        };

        original.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();

        assert_eq!(loaded.project.godot, "4.3-stable".parse::<GodotRelease>().unwrap());
        assert_eq!(loaded.dependency.len(), 1);
        assert_eq!(loaded.dependency[0].name, "gut");
        let map = loaded.dependency[0].map.as_ref().unwrap();
        assert_eq!(map[0].from, "addons/gut");
    }

    #[test]
    fn load_missing_file_returns_error() {
        let result = Config::load(std::path::Path::new("does_not_exist.toml"));
        assert!(result.unwrap_err().to_string().contains("ggg init"));
    }

    #[test]
    fn save_rejects_invalid_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggg.toml");

        let invalid = Config {
            project: Project { godot: "4.3-stable".parse().unwrap() },
            dependency: vec![
                Dependency {
                    name: "gut".into(),
                    git:  "https://github.com/bitwes/Gut.git".into(),
                    rev:  "v9.3.0".into(),
                    map:  None,
                },
                Dependency {
                    name: "gut".into(),
                    git:  "https://github.com/bitwes/Gut.git".into(),
                    rev:  "v9.3.1".into(),
                    map:  None,
                },
            ],
        };

        let result = invalid.save(&path);
        assert!(result.unwrap_err().to_string().contains("gut"));
        // The file must not have been created.
        assert!(!path.exists());
    }
}
