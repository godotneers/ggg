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
    /// `[sync]` - optional sync behaviour settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync: Option<Sync>,
    /// `[[dependency]]` - zero or more addon dependencies.
    ///
    /// Deserialises as an empty `Vec` when no `[[dependency]]` tables are
    /// present, so callers never need to handle a missing key explicitly.
    #[serde(default)]
    pub dependency: Vec<Dependency>,
}

/// The `[sync]` table - optional sync behaviour overrides.
#[derive(Debug, Deserialize, Serialize)]
pub struct Sync {
    /// Glob patterns (e.g. `**/*.import`, `**/*.uid`) matched against
    /// project-relative paths.  Any file whose path matches one of these
    /// patterns is unconditionally overwritten on `ggg sync`, bypassing
    /// conflict detection.
    ///
    /// Use this for files that the Godot engine itself rewrites automatically
    /// (import metadata, UIDs) so that engine-driven changes are never treated
    /// as conflicts.
    #[serde(default)]
    pub force_overwrite: Vec<String>,
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

/// One `[[dependency]]` entry - a single addon sourced from a git repository
/// or a pre-built archive.
///
/// Exactly one of `git` or `url` must be set. Fields specific to one source
/// type are invalid on the other and are rejected by [`Config::validate`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dependency {
    /// Short identifier for this dependency, unique within the file.
    ///
    /// Used in CLI output, the lock file, and as the argument to `ggg remove`.
    pub name: String,

    // --- git source -----------------------------------------------------------
    /// HTTPS or SSH URL of the git repository. Mutually exclusive with `url`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,

    /// Tag, branch, or full commit SHA to check out. Required when `git` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,

    // --- archive source -------------------------------------------------------
    /// HTTPS URL of a pre-built archive (`.zip`, `.tar.gz`, `.tgz`).
    /// Mutually exclusive with `git` and `asset_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Expected SHA-256 hex digest of the downloaded archive. Optional but
    /// strongly recommended: verified against the download on every fetch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,

    /// Number of leading path components to strip from archive entries before
    /// writing to the cache, equivalent to `tar --strip-components`. Default 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strip_components: Option<u32>,

    // --- Godot Asset Library source -------------------------------------------
    /// Numeric asset ID from the Godot Asset Library (godotengine.org).
    /// Mutually exclusive with `git` and `url`.
    ///
    /// The download URL and SHA-256 are resolved at `ggg sync` time via the
    /// asset library API and pinned in `ggg.lock`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<u32>,

    // --- common ---------------------------------------------------------------
    /// Path mappings that control which parts of the source are installed
    /// and where. When absent the entire tree is copied into the project
    /// root as-is.
    ///
    /// See [`MapEntry`] for the per-entry semantics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<Vec<MapEntry>>,
}

/// The source kind of a [`Dependency`], obtained via [`Dependency::kind`].
///
/// Used by pipeline stages that need to branch on dep type (resolve, download,
/// lock file lookup, cache key).
pub enum DepKind<'a> {
    /// Sourced from a git repository.
    Git { git: &'a str, rev: &'a str },
    /// Sourced from a pre-built archive URL.
    Archive {
        url: &'a str,
        sha256: Option<&'a str>,
        strip_components: u32,
    },
    /// Sourced from the Godot Asset Library by numeric asset ID.
    ///
    /// The download URL is resolved at `ggg sync` time and pinned in the lock
    /// file.  The `map` and `strip_components` fields on the parent
    /// [`Dependency`] apply as usual at install time.
    AssetLib { asset_id: u32 },
}

impl Dependency {
    /// Return which kind of source this dependency uses.
    ///
    /// Panics if the dependency has not been validated (i.e. has neither `git`
    /// nor `url`, or has both). Always call [`Config::validate`] before using
    /// this method.
    pub fn kind(&self) -> DepKind<'_> {
        match (&self.git, &self.url, &self.asset_id) {
            (Some(git), None, None) => DepKind::Git {
                git,
                rev: self
                    .rev
                    .as_deref()
                    .expect("git dep missing rev (validate() not called)"),
            },
            (None, Some(url), None) => DepKind::Archive {
                url,
                sha256: self.sha256.as_deref(),
                strip_components: self.strip_components.unwrap_or(0),
            },
            (None, None, Some(id)) => DepKind::AssetLib { asset_id: *id },
            _ => panic!(
                "invalid dep {:?}: must have exactly one of git, url, or asset_id \
                 (call validate() first)",
                self.name
            ),
        }
    }

    /// Convenience constructor for a git dependency.
    pub fn new_git(
        name: impl Into<String>,
        git: impl Into<String>,
        rev: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            git: Some(git.into()),
            rev: Some(rev.into()),
            url: None,
            sha256: None,
            strip_components: None,
            asset_id: None,
            map: None,
        }
    }

    /// Convenience constructor for an archive dependency.
    pub fn new_archive(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            git: None,
            rev: None,
            url: Some(url.into()),
            sha256: None,
            strip_components: None,
            asset_id: None,
            map: None,
        }
    }

    /// Convenience constructor for a Godot Asset Library dependency.
    pub fn new_asset_lib(name: impl Into<String>, asset_id: u32) -> Self {
        Self {
            name: name.into(),
            git: None,
            rev: None,
            url: None,
            sha256: None,
            strip_components: None,
            asset_id: Some(asset_id),
            map: None,
        }
    }

    /// Validate the source fields of this single dependency entry.
    fn validate_source(&self) -> Result<()> {
        let source_count = [self.git.is_some(), self.url.is_some(), self.asset_id.is_some()]
            .iter()
            .filter(|&&b| b)
            .count();

        if source_count > 1 {
            anyhow::bail!(
                "dependency {:?}: 'git', 'url', and 'asset_id' are mutually exclusive; \
                 set exactly one",
                self.name
            );
        }

        if source_count == 0 {
            anyhow::bail!(
                "dependency {:?}: must have exactly one of 'git', 'url', or 'asset_id'",
                self.name
            );
        }

        if self.git.is_some() {
            if self.rev.is_none() {
                anyhow::bail!(
                    "dependency {:?}: 'git' dependencies require a 'rev' field",
                    self.name
                );
            }
            if self.sha256.is_some() {
                anyhow::bail!(
                    "dependency {:?}: 'sha256' is only valid for archive ('url') dependencies",
                    self.name
                );
            }
            if self.strip_components.is_some() {
                anyhow::bail!(
                    "dependency {:?}: 'strip_components' is only valid for archive ('url') dependencies",
                    self.name
                );
            }
        }

        if let Some(url) = &self.url {
            if self.rev.is_some() {
                anyhow::bail!(
                    "dependency {:?}: 'rev' is only valid for 'git' dependencies",
                    self.name
                );
            }
            let supported =
                url.ends_with(".zip") || url.ends_with(".tar.gz") || url.ends_with(".tgz");
            if !supported {
                anyhow::bail!(
                    "dependency {:?}: unrecognised archive format in URL {:?}; \
                     supported extensions: .zip, .tar.gz, .tgz",
                    self.name,
                    url
                );
            }
        }

        if self.asset_id.is_some() {
            if self.rev.is_some() {
                anyhow::bail!(
                    "dependency {:?}: 'rev' is only valid for 'git' dependencies",
                    self.name
                );
            }
            if self.sha256.is_some() {
                anyhow::bail!(
                    "dependency {:?}: 'sha256' is not valid for asset library dependencies \
                     (the hash is recorded automatically in ggg.lock)",
                    self.name
                );
            }
        }

        Ok(())
    }
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    /// Enforces:
    /// - All dependency names are unique.
    /// - Each dependency has exactly one source (`git` or `url`), with the
    ///   correct accompanying fields.
    pub fn validate(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for dep in &self.dependency {
            if !seen.insert(&dep.name) {
                anyhow::bail!("duplicate dependency name: \"{}\"", dep.name);
            }
            dep.validate_source()?;
        }
        Ok(())
    }

    /// Validate, serialise, and write this config to `path`, overwriting any
    /// existing file.
    ///
    /// Validation runs before any I/O, so an invalid config is rejected
    /// without touching the file.
    ///
    /// When writing to an existing file the `[[dependency]]` section is
    /// spliced into the original `toml_edit` document so that comments and
    /// formatting elsewhere in the file are preserved. For new files the
    /// config is serialised fresh.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;

        // Always produce a fresh serialisation - this handles all field types
        // (including `map` inline tables) without manual toml_edit construction.
        let fresh = toml_edit::ser::to_string_pretty(self).context("failed to serialize config")?;

        if !path.exists() {
            return std::fs::write(path, fresh)
                .with_context(|| format!("failed to write {}", path.display()));
        }

        // Existing file: load the original document (preserving comments), then
        // replace only the `dependency` section with the freshly serialised one.
        let original = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut doc: toml_edit::DocumentMut = original
            .parse()
            .with_context(|| format!("failed to parse {}", path.display()))?;
        let fresh_doc: toml_edit::DocumentMut = fresh
            .parse()
            .context("internal error: freshly serialized config is not valid TOML")?;

        doc.remove("dependency");
        if let Some(deps) = fresh_doc.get("dependency") {
            doc.insert("dependency", deps.clone());
        }

        std::fs::write(path, doc.to_string())
            .with_context(|| format!("failed to write {}", path.display()))
    }

    /// Find a dependency by its name.
    pub fn get_dependency(&self, name: &str) -> Option<&Dependency> {
        self.dependency.iter().find(|dep| dep.name == name)
    }

    /// Removes a dependency with the given name if it exists. Otherwise does
    /// nothing.
    pub fn remove_dependency(&mut self, name: &str) {
        self.dependency.retain(|dep| dep.name != name);
    }

    /// Returns true if the config has a dependency with the given name.
    pub fn has_dependency(&self, name: &str) -> bool {
        self.dependency.iter().any(|dep| dep.name == name)
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
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"
        "#,
        );

        assert_eq!(
            config.project.godot,
            "4.3-stable".parse::<GodotRelease>().unwrap()
        );
        assert!(config.dependency.is_empty());
    }

    #[test]
    fn parse_dependency_without_map() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
        "#,
        );

        assert_eq!(config.dependency.len(), 1);
        let dep = &config.dependency[0];
        assert_eq!(dep.name, "gut");
        assert_eq!(dep.rev.as_deref(), Some("v9.3.0"));
        assert!(dep.map.is_none());
    }

    #[test]
    fn parse_dependency_with_symmetric_map() {
        // When `to` is omitted, the entry is still valid - the field is None.
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
            map  = [{ from = "addons/gut" }]
        "#,
        );

        let map = config.dependency[0].map.as_ref().unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map[0].from, "addons/gut");
        assert!(map[0].to.is_none());
    }

    #[test]
    fn parse_dependency_with_renamed_map() {
        let config = parse(
            r#"
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
        "#,
        );

        let map = config.dependency[0].map.as_ref().unwrap();
        assert_eq!(map.len(), 2);
        assert!(map[0].to.is_none());
        assert_eq!(map[1].to.as_deref(), Some("examples/gut"));
    }

    #[test]
    fn parse_multiple_dependencies() {
        let config = parse(
            r#"
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
        "#,
        );

        assert_eq!(config.dependency.len(), 2);
        assert_eq!(config.dependency[1].name, "phantom-camera");
    }

    #[test]
    fn parse_archive_dependency() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name             = "debug_draw_3d"
            url              = "https://example.com/debug_draw_3d.zip"
            sha256           = "abc123"
            strip_components = 1
            map              = [{ from = "addons/debug_draw_3d" }]
        "#,
        );
        assert_eq!(config.dependency.len(), 1);
        let dep = &config.dependency[0];
        assert_eq!(
            dep.url.as_deref(),
            Some("https://example.com/debug_draw_3d.zip")
        );
        assert_eq!(dep.sha256.as_deref(), Some("abc123"));
        assert_eq!(dep.strip_components, Some(1));
        assert!(dep.git.is_none());
        assert!(dep.rev.is_none());
    }

    // --- required field errors ---------------------------------------------

    #[test]
    fn parse_missing_godot_field_fails() {
        let result = toml_edit::de::from_str::<Config>(
            r#"
            [project]
        "#,
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("missing field `godot`")
        );
    }

    #[test]
    fn parse_missing_dependency_name_fails() {
        let result = toml_edit::de::from_str::<Config>(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            git = "https://github.com/bitwes/Gut.git"
            rev = "v9.3.0"
        "#,
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("missing field `name`")
        );
    }

    #[test]
    fn validate_rejects_dep_with_neither_git_nor_url() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
        "#,
        );
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("git") || err.contains("url"), "err was: {err}");
    }

    #[test]
    fn validate_rejects_git_dep_without_rev() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
        "#,
        );
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("rev"), "err was: {err}");
    }

    #[test]
    fn validate_rejects_git_dep_with_sha256() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name   = "gut"
            git    = "https://github.com/bitwes/Gut.git"
            rev    = "main"
            sha256 = "abc"
        "#,
        );
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_archive_dep_with_rev() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "foo"
            url  = "https://example.com/foo.zip"
            rev  = "main"
        "#,
        );
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_dep_with_both_git_and_url() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "foo"
            git  = "https://example.com/foo.git"
            rev  = "main"
            url  = "https://example.com/foo.zip"
        "#,
        );
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_unknown_archive_extension() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "foo"
            url  = "https://example.com/foo.rar"
        "#,
        );
        let err = config.validate().unwrap_err().to_string();
        assert!(
            err.contains("extension") || err.contains("format"),
            "err was: {err}"
        );
    }

    #[test]
    fn parse_missing_map_entry_from_fails() {
        let result = toml_edit::de::from_str::<Config>(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
            map  = [{ to = "addons/gut" }]
        "#,
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("missing field `from`")
        );
    }

    // --- validation --------------------------------------------------------

    #[test]
    fn validate_rejects_duplicate_dependency_names() {
        let config = parse(
            r#"
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
        "#,
        );

        assert!(config.validate().unwrap_err().to_string().contains("gut"));
    }

    #[test]
    fn validate_accepts_unique_dependency_names() {
        let config = parse(
            r#"
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
        "#,
        );

        assert!(config.validate().is_ok());
    }

    // --- serialization -----------------------------------------------------

    #[test]
    fn absent_map_is_not_serialized() {
        // Optional fields set to None must not appear in the output at all,
        // so the written file stays clean and round-trips back correctly.
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
        "#,
        );

        let output = serialize(&config);
        assert!(!output.contains("map"));
    }

    #[test]
    fn absent_map_entry_to_is_not_serialized() {
        // Same principle for the `to` field inside a map entry.
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://github.com/bitwes/Gut.git"
            rev  = "v9.3.0"
            map  = [{ from = "addons/gut" }]
        "#,
        );

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
            project: Project {
                godot: "4.3-stable".parse().unwrap(),
            },
            sync: None,
            dependency: vec![{
                let mut d =
                    Dependency::new_git("gut", "https://github.com/bitwes/Gut.git", "v9.3.0");
                d.map = Some(vec![MapEntry {
                    from: "addons/gut".into(),
                    to: None,
                }]);
                d
            }],
        };

        original.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();

        assert_eq!(
            loaded.project.godot,
            "4.3-stable".parse::<GodotRelease>().unwrap()
        );
        assert_eq!(loaded.dependency.len(), 1);
        assert_eq!(loaded.dependency[0].name, "gut");
        assert_eq!(loaded.dependency[0].rev.as_deref(), Some("v9.3.0"));
        let map = loaded.dependency[0].map.as_ref().unwrap();
        assert_eq!(map[0].from, "addons/gut");
    }

    #[test]
    fn load_missing_file_returns_error() {
        let result = Config::load(Path::new("does_not_exist.toml"));
        assert!(result.unwrap_err().to_string().contains("ggg init"));
    }

    #[test]
    fn save_rejects_invalid_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggg.toml");

        let invalid = Config {
            project: Project {
                godot: "4.3-stable".parse().unwrap(),
            },
            sync: None,
            dependency: vec![
                Dependency::new_git("gut", "https://github.com/bitwes/Gut.git", "v9.3.0"),
                Dependency::new_git("gut", "https://github.com/bitwes/Gut.git", "v9.3.1"),
            ],
        };

        let result = invalid.save(&path);
        assert!(result.unwrap_err().to_string().contains("gut"));
        // The file must not have been created.
        assert!(!path.exists());
    }

    #[test]
    fn save_preserves_comments_in_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggg.toml");

        std::fs::write(
            &path,
            "# top-level comment\n[project]\ngodot = \"4.3-stable\"\n",
        )
        .unwrap();

        let mut config = Config::load(&path).unwrap();
        config.dependency.push(Dependency::new_git(
            "gut",
            "https://github.com/bitwes/Gut.git",
            "v9.3.0",
        ));
        config.save(&path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("# top-level comment"),
            "comment was stripped"
        );
    }

    #[test]
    fn save_appends_dependency_to_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggg.toml");

        std::fs::write(&path, "[project]\ngodot = \"4.3-stable\"\n").unwrap();

        let mut config = Config::load(&path).unwrap();
        config.dependency.push(Dependency::new_git(
            "gut",
            "https://github.com/bitwes/Gut.git",
            "v9.3.0",
        ));
        config.save(&path).unwrap();

        let reloaded = Config::load(&path).unwrap();
        assert_eq!(reloaded.dependency.len(), 1);
        assert_eq!(reloaded.dependency[0].name, "gut");
    }

    #[test]
    fn save_removes_dependency_from_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggg.toml");

        std::fs::write(
            &path,
            r#"[project]
godot = "4.3-stable"

[[dependency]]
name = "gut"
git  = "https://github.com/bitwes/Gut.git"
rev  = "v9.3.0"

[[dependency]]
name = "phantom-camera"
git  = "https://github.com/ramokz/phantom-camera.git"
rev  = "main"
"#,
        )
        .unwrap();

        let mut config = Config::load(&path).unwrap();
        config.dependency.retain(|d| d.name != "gut");
        config.save(&path).unwrap();

        let reloaded = Config::load(&path).unwrap();
        assert_eq!(reloaded.dependency.len(), 1);
        assert_eq!(reloaded.dependency[0].name, "phantom-camera");
    }
}
