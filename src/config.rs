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
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use crate::godot::release::GodotRelease;
use crate::utils::validation::{
    validate_archive_url, validate_asset_store_slug, validate_version_tag,
};

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
    /// Deserializes as an empty `Vec` when no `[[dependency]]` tables are
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

fn is_false(v: &bool) -> bool {
    !v
}

/// The `[project]` table.
#[derive(Debug, Deserialize, Serialize)]
pub struct Project {
    /// The exact Godot build to use, e.g. `"4.3-stable"` or `"4.3-stable-mono"`.
    ///
    /// `ggg sync` downloads this binary if it is not already cached.
    /// `ggg edit` and `ggg run` invoke it.
    pub godot: GodotRelease,

    /// Whether to download and install export templates alongside the Godot binary.
    ///
    /// Set to `true` during `ggg init` when the user opts in, or by passing
    /// `--with-export-templates` to any command that supports it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub export_templates: bool,
}

/// One `[[dependency]]` entry - a single addon sourced from a git repository,
/// a pre-built archive, the Godot Asset Library, or the Godot Asset Store.
///
/// [`Source`] enforces that exactly one source is set and that
/// source-specific fields (`rev`, `sha256`, ...) only appear on the source
/// they apply to - invalid combinations are rejected at parse time rather
/// than by a separate validation pass.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dependency {
    /// Short identifier for this dependency, unique within the file.
    ///
    /// Used in CLI output, the lock file, and as the argument to `ggg remove`.
    pub name: String,

    /// Which source this dependency is fetched from, and the fields specific
    /// to that source. Flattened, so e.g. `git`/`rev` appear directly in the
    /// `[[dependency]]` table rather than under a nested `[dependency.source]`.
    #[serde(flatten)]
    pub source: Source,

    // --- common ---------------------------------------------------------------
    /// Path mappings that control which parts of the source are installed
    /// and where. When absent the entire tree is copied into the project
    /// root as-is.
    ///
    /// See [`MapEntry`] for the per-entry semantics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<Vec<MapEntry>>,

    /// Glob patterns matched against destination paths to exclude from
    /// installation. Applied after `map`, so patterns refer to the
    /// post-mapping destination paths (e.g. `addons/gut/examples/**`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,
}

/// A parsed reference to an asset in the Godot Asset Store, of the form
/// `publisher_slug/asset_slug:version`.
///
/// Slugs are restricted to lowercase alphanumerics and `-`; `version` follows
/// docker-tag syntax (`[A-Za-z0-9][A-Za-z0-9._-]*`) and is required, so a store
/// dep always pins a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetStoreRef {
    pub publisher: String,
    pub asset: String,
    pub version: String,
}

impl fmt::Display for AssetStoreRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}:{}", self.publisher, self.asset, self.version)
    }
}

impl FromStr for AssetStoreRef {
    type Err = anyhow::Error;

    /// Parse an asset store reference of the form
    /// `"publisher_slug/asset_slug:version"`.
    fn from_str(s: &str) -> Result<Self> {
        let (path, version) = s.split_once(':').with_context(|| {
            format!(
                "invalid asset store reference {s:?}: expected publisher_slug/asset_slug:version"
            )
        })?;
        let (publisher, asset) = path.split_once('/').with_context(|| {
            format!(
                "invalid asset store reference {s:?}: expected publisher_slug/asset_slug:version"
            )
        })?;
        validate_asset_store_slug("publisher", publisher)
            .map_err(|e| anyhow::anyhow!("invalid asset store reference {s:?}: {e}"))?;
        validate_asset_store_slug("asset", asset)
            .map_err(|e| anyhow::anyhow!("invalid asset store reference {s:?}: {e}"))?;
        validate_version_tag(version)
            .map_err(|e| anyhow::anyhow!("invalid asset store reference {s:?}: {e}"))?;
        Ok(Self {
            publisher: publisher.to_string(),
            asset: asset.to_string(),
            version: version.to_string(),
        })
    }
}

impl Serialize for AssetStoreRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AssetStoreRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// The source a [`Dependency`] is fetched from.
///
/// Each variant carries exactly the fields that apply to it, so a
/// `Dependency` can never be built (from Rust or from `ggg.toml`) with
/// conflicting or misplaced source fields - there is no runtime "which
/// combination of fields is set" check left to fall out of sync with reality.
///
/// Deserialized via a hand-written [`Deserialize`] impl rather than
/// `#[serde(untagged)]`: an untagged enum picks the first variant whose
/// required fields are present and silently ignores any extra fields from
/// other variants, so e.g. `git` + `url` both set would silently deserialize
/// as `Git` with `url` dropped. The manual impl collects every possible
/// field first, then explicitly rejects any combination other than "exactly
/// one source".
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Source {
    /// Sourced from a git repository.
    Git { git: String, rev: String },
    /// Sourced from a pre-built archive URL.
    Archive {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        strip_components: Option<u32>,
    },
    /// Sourced from the Godot Asset Library by numeric asset ID.
    AssetLib {
        asset_library_id: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        strip_components: Option<u32>,
    },
    /// Sourced from the Godot Asset Store.
    AssetStore {
        asset_store_asset: AssetStoreRef,
        #[serde(skip_serializing_if = "Option::is_none")]
        strip_components: Option<u32>,
    },
}

/// The four source kinds a [`Dependency`] (or a `ggg.lock` entry) can record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Git,
    Archive,
    AssetLib,
    AssetStore,
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceKind::Git => write!(f, "git"),
            SourceKind::Archive => write!(f, "archive"),
            SourceKind::AssetLib => write!(f, "asset-lib"),
            SourceKind::AssetStore => write!(f, "asset-store"),
        }
    }
}

/// Every field any [`Source`] variant can carry, collected in a single pass
/// over the `[[dependency]]` table before [`Source::deserialize`] decides
/// which variant applies.
#[derive(Debug, Default, Deserialize)]
struct RawSource {
    #[serde(default)]
    git: Option<String>,
    #[serde(default)]
    rev: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    strip_components: Option<u32>,
    #[serde(default, alias = "asset_id")]
    asset_library_id: Option<u32>,
    #[serde(default)]
    asset_store_asset: Option<AssetStoreRef>,
}

impl<'de> Deserialize<'de> for Source {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let raw = RawSource::deserialize(deserializer)?;

        let source_count = [
            raw.git.is_some(),
            raw.url.is_some(),
            raw.asset_library_id.is_some(),
            raw.asset_store_asset.is_some(),
        ]
        .into_iter()
        .filter(|&b| b)
        .count();

        if source_count == 0 {
            return Err(D::Error::custom(
                "must have exactly one of 'git', 'url', 'asset_library_id', or 'asset_store_asset'",
            ));
        }
        if source_count > 1 {
            return Err(D::Error::custom(
                "'git', 'url', 'asset_library_id', and 'asset_store_asset' are mutually \
                 exclusive; set exactly one",
            ));
        }

        if let Some(git) = raw.git {
            if raw.sha256.is_some() {
                return Err(D::Error::custom(
                    "'sha256' is only valid for 'url' dependencies",
                ));
            }
            if raw.strip_components.is_some() {
                return Err(D::Error::custom(
                    "'strip_components' is only valid for 'url', 'asset_library_id', or \
                     'asset_store_asset' dependencies",
                ));
            }
            let rev = raw
                .rev
                .ok_or_else(|| D::Error::custom("'git' dependencies require a 'rev' field"))?;
            return Ok(Source::Git { git, rev });
        }

        if raw.rev.is_some() {
            return Err(D::Error::custom(
                "'rev' is only valid for 'git' dependencies",
            ));
        }

        if let Some(url) = raw.url {
            validate_archive_url(&url).map_err(D::Error::custom)?;
            return Ok(Source::Archive {
                url,
                sha256: raw.sha256,
                strip_components: raw.strip_components,
            });
        }

        if raw.sha256.is_some() {
            return Err(D::Error::custom(
                "'sha256' is not valid for asset library or asset store dependencies \
                 (the hash is recorded automatically in ggg.lock)",
            ));
        }

        if let Some(asset_library_id) = raw.asset_library_id {
            return Ok(Source::AssetLib {
                asset_library_id,
                strip_components: raw.strip_components,
            });
        }

        Ok(Source::AssetStore {
            asset_store_asset: raw
                .asset_store_asset
                .expect("checked above: exactly one source field is present"),
            strip_components: raw.strip_components,
        })
    }
}

impl Dependency {
    /// Construct a dependency from a fully-specified [`Source`].
    ///
    /// `map` and `exclude` are optional; pass `None` to omit them. Callers
    /// build the source variant themselves:
    ///
    /// ```
    /// use ggg::config::{Dependency, Source};
    ///
    /// let git = Dependency::new(
    ///     "gut",
    ///     Source::Git {
    ///         git: "https://example.com/gut.git".into(),
    ///         rev: "v9.3.0".into(),
    ///     },
    ///     None,
    ///     None,
    /// );
    /// ```
    pub fn new(
        name: impl Into<String>,
        source: Source,
        map: Option<Vec<MapEntry>>,
        exclude: Option<Vec<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            source,
            map,
            exclude,
        }
    }

    /// Which [`SourceKind`] this dependency is sourced from.
    pub fn kind(&self) -> SourceKind {
        match &self.source {
            Source::Git { .. } => SourceKind::Git,
            Source::Archive { .. } => SourceKind::Archive,
            Source::AssetLib { .. } => SourceKind::AssetLib,
            Source::AssetStore { .. } => SourceKind::AssetStore,
        }
    }

    /// Validate this dependency's source fields.
    ///
    /// Most invalid combinations are already unrepresentable thanks to
    /// [`Source`]'s custom `Deserialize` impl, so this only re-checks what a
    /// `Dependency` built directly in Rust (bypassing deserialization) could
    /// still get wrong - namely an unsupported archive URL extension.
    fn validate_source(&self) -> Result<()> {
        if let Source::Archive { url, .. } = &self.source {
            validate_archive_url(url).with_context(|| format!("dependency {:?}", self.name))?;
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
    /// Read and deserialize a `ggg.toml` file from `path`, then validate it.
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

    /// Find a dependency by its name, mutably.
    pub(crate) fn get_dependency_mut(&mut self, name: &str) -> Option<&mut Dependency> {
        self.dependency.iter_mut().find(|dep| dep.name == name)
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
            git  = "https://example.com/gut.git"
            rev  = "v9.3.0"
        "#,
        );

        assert_eq!(config.dependency.len(), 1);
        let dep = &config.dependency[0];
        assert_eq!(dep.name, "gut");
        let Source::Git { rev, .. } = &dep.source else {
            panic!("expected Git source");
        };
        assert_eq!(rev, "v9.3.0");
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
            git  = "https://example.com/gut.git"
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
            git  = "https://example.com/gut.git"
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
            git  = "https://example.com/gut.git"
            rev  = "v9.3.0"

            [[dependency]]
            name = "phantom-camera"
            git  = "https://example.com/phantom-camera.git"
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
        let Source::Archive {
            url,
            sha256,
            strip_components,
        } = &dep.source
        else {
            panic!("expected Archive source");
        };
        assert_eq!(url, "https://example.com/debug_draw_3d.zip");
        assert_eq!(sha256.as_deref(), Some("abc123"));
        assert_eq!(*strip_components, Some(1));
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
            git = "https://example.com/gut.git"
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
    fn parse_rejects_dep_with_neither_git_nor_url() {
        let err = toml_edit::de::from_str::<Config>(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
        "#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("git") || err.contains("url"), "err was: {err}");
    }

    #[test]
    fn parse_rejects_git_dep_without_rev() {
        let err = toml_edit::de::from_str::<Config>(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://example.com/gut.git"
        "#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("rev"), "err was: {err}");
    }

    #[test]
    fn parse_rejects_git_dep_with_sha256() {
        let result = toml_edit::de::from_str::<Config>(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name   = "gut"
            git    = "https://example.com/gut.git"
            rev    = "main"
            sha256 = "abc"
        "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_archive_dep_with_rev() {
        let result = toml_edit::de::from_str::<Config>(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "foo"
            url  = "https://example.com/foo.zip"
            rev  = "main"
        "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_dep_with_both_git_and_url() {
        let result = toml_edit::de::from_str::<Config>(
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
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_unknown_archive_extension() {
        let err = toml_edit::de::from_str::<Config>(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "foo"
            url  = "https://example.com/foo.rar"
        "#,
        )
        .unwrap_err()
        .to_string();
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
            git  = "https://example.com/gut.git"
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
            git  = "https://example.com/gut.git"
            rev  = "v9.3.0"

            [[dependency]]
            name = "gut"
            git  = "https://example.com/gut.git"
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
            git  = "https://example.com/gut.git"
            rev  = "v9.3.0"

            [[dependency]]
            name = "phantom-camera"
            git  = "https://example.com/phantom-camera.git"
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
            git  = "https://example.com/gut.git"
            rev  = "v9.3.0"
        "#,
        );

        let output = serialize(&config);
        assert!(!output.contains("map"));
    }

    #[test]
    fn parse_dependency_with_exclude() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name    = "gut"
            git     = "https://example.com/gut.git"
            rev     = "v9.3.0"
            exclude = ["addons/gut/examples/**", "**/*.test.gd"]
        "#,
        );
        let exc = config.dependency[0].exclude.as_ref().unwrap();
        assert_eq!(exc.len(), 2);
        assert_eq!(exc[0], "addons/gut/examples/**");
        assert_eq!(exc[1], "**/*.test.gd");
    }

    #[test]
    fn absent_exclude_is_not_serialized() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "https://example.com/gut.git"
            rev  = "v9.3.0"
        "#,
        );
        let output = serialize(&config);
        assert!(!output.contains("exclude"));
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
            git  = "https://example.com/gut.git"
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
                export_templates: false,
            },
            sync: None,
            dependency: vec![Dependency::new(
                "gut",
                Source::Git {
                    git: "https://example.com/gut.git".to_owned(),
                    rev: "v9.3.0".to_owned(),
                },
                Some(vec![MapEntry {
                    from: "addons/gut".into(),
                    to: None,
                }]),
                None,
            )],
        };

        original.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();

        assert_eq!(
            loaded.project.godot,
            "4.3-stable".parse::<GodotRelease>().unwrap()
        );
        assert_eq!(loaded.dependency.len(), 1);
        assert_eq!(loaded.dependency[0].name, "gut");
        let Source::Git { rev, .. } = &loaded.dependency[0].source else {
            panic!("expected Git source");
        };
        assert_eq!(rev, "v9.3.0");
        let map = loaded.dependency[0].map.as_ref().unwrap();
        assert_eq!(map[0].from, "addons/gut");
    }

    #[test]
    fn legacy_asset_id_alias_loads_and_round_trips() {
        // Pre-rename ggg.toml files used `asset_id`. They must still load, and
        // be rewritten as `asset_library_id` on the next save.
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name     = "dialogic"
            asset_id = 1216
        "#,
        );

        assert_eq!(config.dependency.len(), 1);
        let Source::AssetLib {
            asset_library_id, ..
        } = &config.dependency[0].source
        else {
            panic!("expected AssetLib source");
        };
        assert_eq!(*asset_library_id, 1216);

        let output = serialize(&config);
        assert!(output.contains("asset_library_id = 1216"));
        assert!(!output.contains("asset_id"));
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
                export_templates: false,
            },
            sync: None,
            dependency: vec![
                Dependency::new(
                    "gut",
                    Source::Git {
                        git: "https://example.com/gut.git".to_owned(),
                        rev: "v9.3.0".to_owned(),
                    },
                    None,
                    None,
                ),
                Dependency::new(
                    "gut",
                    Source::Git {
                        git: "https://example.com/gut.git".to_owned(),
                        rev: "v9.3.1".to_owned(),
                    },
                    None,
                    None,
                ),
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
        config.dependency.push(Dependency::new(
            "gut",
            Source::Git {
                git: "https://example.com/gut.git".to_owned(),
                rev: "v9.3.0".to_owned(),
            },
            None,
            None,
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
        config.dependency.push(Dependency::new(
            "gut",
            Source::Git {
                git: "https://example.com/gut.git".to_owned(),
                rev: "v9.3.0".to_owned(),
            },
            None,
            None,
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
git  = "https://example.com/gut.git"
rev  = "v9.3.0"

[[dependency]]
name = "phantom-camera"
git  = "https://example.com/phantom-camera.git"
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

    // --- asset store --------------------------------------------------------

    #[test]
    fn asset_store_ref_parses_valid_specs() {
        let cases = [
            ("publisher/asset:1.0.0", "publisher", "asset", "1.0.0"),
            (
                "my-publisher/my-asset:1.0.0-beta.1",
                "my-publisher",
                "my-asset",
                "1.0.0-beta.1",
            ),
            ("abc/def:v2_3", "abc", "def", "v2_3"),
            ("abc/def:1.0", "abc", "def", "1.0"),
            ("a-b-c/a-b:9Z.X_y-z", "a-b-c", "a-b", "9Z.X_y-z"),
        ];
        for (spec, publisher, asset, version) in cases {
            let parsed = spec.parse::<AssetStoreRef>().unwrap();
            assert_eq!(parsed.publisher, publisher, "spec {spec:?}");
            assert_eq!(parsed.asset, asset, "spec {spec:?}");
            assert_eq!(parsed.version, version, "spec {spec:?}");
        }
    }

    #[test]
    fn asset_store_ref_accepts_slug_length_boundaries() {
        let short = format!("{}/asset:v1", "a".repeat(3));
        assert!(short.parse::<AssetStoreRef>().is_ok());

        let long = format!("{}/asset:v1", "a".repeat(256));
        assert!(long.parse::<AssetStoreRef>().is_ok());

        let too_short = format!("{}/asset:v1", "a".repeat(2));
        assert!(too_short.parse::<AssetStoreRef>().is_err());

        let too_long = format!("{}/asset:v1", "a".repeat(257));
        assert!(too_long.parse::<AssetStoreRef>().is_err());
    }

    #[test]
    fn asset_store_ref_rejects_malformed_specs() {
        let bad = [
            "",
            "publisher",
            "publisher/asset",
            "publisher/asset:",
            "/asset:1.0.0",
            "publisher/:1.0.0",
            "Publisher/asset:1.0.0",
            "publisher/Asset:1.0.0",
            "publish_er/asset:1.0.0",
            "publisher/as_sets:1.0.0",
            "publisher/asset.tgz:1.0.0",
            "publisher/asset:main/other:1.0.0",
            "publisher/asset:.1.0.0",
            "publisher/asset:-1",
            "publisher/asset:1.0 0",
            "publisher/asset:1*0",
            "publisher:asset:1.0.0",
            "publisher/asset:",
        ];
        for spec in bad {
            assert!(
                spec.parse::<AssetStoreRef>().is_err(),
                "spec {spec:?} should not parse"
            );
        }
    }

    #[test]
    fn asset_store_ref_error_messages_are_clear() {
        let err = "publisher/asset"
            .parse::<AssetStoreRef>()
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("expected publisher_slug/asset_slug:version"),
            "err was: {err}"
        );

        let err = "PUB/asset:1.0.0"
            .parse::<AssetStoreRef>()
            .unwrap_err()
            .to_string();
        assert!(err.contains("publisher slug"), "err was: {err}");
    }

    #[test]
    fn parse_asset_store_dependency() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name              = "my-store-addon"
            asset_store_asset = "publisher/my-addon:1.2.3"
            strip_components  = 1
            map               = [{ from = "addons/my-addon" }]
        "#,
        );
        assert_eq!(config.dependency.len(), 1);
        let dep = &config.dependency[0];
        assert_eq!(dep.name, "my-store-addon");
        let Source::AssetStore {
            asset_store_asset,
            strip_components,
        } = &dep.source
        else {
            panic!("expected AssetStore source");
        };
        assert_eq!(asset_store_asset.to_string(), "publisher/my-addon:1.2.3");
        assert_eq!(*strip_components, Some(1));
        assert!(dep.map.is_some());
    }

    #[test]
    fn parse_rejects_malformed_asset_store_spec() {
        let result = toml_edit::de::from_str::<Config>(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name              = "my-store-addon"
            asset_store_asset = "not-a-valid/spec"
        "#,
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("expected publisher_slug/asset_slug:version"),
            "err was: {err}"
        );
    }

    #[test]
    fn parse_rejects_asset_store_combined_with_other_sources() {
        for extra in [
            r#"git  = "https://example.com/foo.git"
            rev  = "main""#,
            r#"url  = "https://example.com/foo.zip""#,
            "asset_library_id = 1216",
        ] {
            let toml = format!(
                r#"
                [project]
                godot = "4.3-stable"

                [[dependency]]
                name              = "foo"
                asset_store_asset = "publisher/asset:1.0.0"
                {extra}
            "#
            );
            let result = toml_edit::de::from_str::<Config>(&toml);
            let err = result.unwrap_err().to_string();
            assert!(err.contains("exclusive"), "err was: {err}");
        }
    }

    #[test]
    fn parse_rejects_asset_store_with_wrong_aux_fields() {
        let result = toml_edit::de::from_str::<Config>(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name              = "foo"
            asset_store_asset = "publisher/asset:1.0.0"
            sha256            = "abc"
        "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn ssh_style_git_url_still_classifies_as_git() {
        // SSH/scp-style URLs contain dots in the host and a colon before the
        // path; they must stay a git source and never be confused with the
        // asset store grammar.
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name = "gut"
            git  = "git@github.com:godotneers/gut.git"
            rev  = "v9.3.0"
        "#,
        );
        let Source::Git { git, rev } = &config.dependency[0].source else {
            panic!("expected Git source");
        };
        assert_eq!(git, "git@github.com:godotneers/gut.git");
        assert_eq!(rev, "v9.3.0");
    }

    #[test]
    fn numeric_asset_library_id_still_classifies_as_asset_lib() {
        let config = parse(
            r#"
            [project]
            godot = "4.3-stable"

            [[dependency]]
            name             = "dialogic"
            asset_library_id = 1216
        "#,
        );
        let Source::AssetLib {
            asset_library_id, ..
        } = &config.dependency[0].source
        else {
            panic!("expected AssetLib source");
        };
        assert_eq!(*asset_library_id, 1216);
    }

    #[test]
    fn asset_store_dep_round_trips_through_toml_edit() {
        let config = Config {
            project: Project {
                godot: "4.3-stable".parse().unwrap(),
                export_templates: false,
            },
            sync: None,
            dependency: vec![Dependency::new(
                "my-addon",
                Source::AssetStore {
                    asset_store_asset: "publisher/my-addon:1.2.3".parse().unwrap(),
                    strip_components: None,
                },
                None,
                None,
            )],
        };

        let output = serialize(&config);
        assert!(
            output.contains(r#"asset_store_asset = "publisher/my-addon:1.2.3""#),
            "output was: {output}"
        );
        assert!(!output.contains("git"), "output was: {output}");

        let reloaded = parse(&output);
        assert_eq!(reloaded.dependency.len(), 1);
        let Source::AssetStore {
            asset_store_asset, ..
        } = &reloaded.dependency[0].source
        else {
            panic!("expected AssetStore source");
        };
        assert_eq!(asset_store_asset.to_string(), "publisher/my-addon:1.2.3");
    }
}
