//! Reading metadata from an existing `project.godot` file.
//!
//! Godot stores project metadata in a custom INI-like format. The line we care
//! about is in the `[application]` section:
//!
//! ```text
//! config/features=PackedStringArray("4.3", "C#", "Forward Plus")
//! ```
//!
//! The first element of the array is always the Godot version series. `"C#"`
//! being present indicates a Mono (C#) build.

use std::path::Path;

use anyhow::{Context, Result};

use super::release::GodotVersion;

/// Metadata extracted from a `project.godot` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    /// Godot version series as recorded in the project file.
    pub version: GodotVersion,
    /// Whether the project uses Mono (C#).
    pub mono: bool,
}

/// Read a `project.godot` file and extract the Godot version and mono flag.
///
/// Returns `None` if the file does not contain a `config/features` line (e.g.
/// very old projects or manually created files).
pub fn read_project_info(path: &Path) -> Result<Option<ProjectInfo>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(parse_project_info(&content))
}

/// Parse the contents of a `project.godot` file and extract [`ProjectInfo`].
///
/// Separated from [`read_project_info`] so it can be tested without touching
/// the filesystem.
pub fn parse_project_info(content: &str) -> Option<ProjectInfo> {
    let line = content
        .lines()
        .find(|l| l.starts_with("config/features=PackedStringArray("))?;

    // Everything inside the outer parentheses.
    let inner = line
        .strip_prefix("config/features=PackedStringArray(")?
        .strip_suffix(')')?;

    // Split on commas and collect the quoted string values.
    let items: Vec<&str> = inner
        .split(',')
        .map(|s| s.trim().trim_matches('"'))
        .collect();

    // The first item is always the version series.
    let version: GodotVersion = items.first()?.parse().ok()?;

    let mono = items.contains(&"C#");

    Some(ProjectInfo { version, mono })
}

// --- tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_project() {
        let content = r#"[application]

config/name="My Game"
config/features=PackedStringArray("4.3", "Forward Plus")
"#;
        let info = parse_project_info(content).unwrap();
        assert_eq!(info.version, GodotVersion::new(4, 3, 0));
        assert!(!info.mono);
    }

    #[test]
    fn parses_mono_project() {
        let content = r#"[application]

config/name="My Game"
config/features=PackedStringArray("4.3", "C#", "Forward Plus")
"#;
        let info = parse_project_info(content).unwrap();
        assert_eq!(info.version, GodotVersion::new(4, 3, 0));
        assert!(info.mono);
    }

    #[test]
    fn parses_version_with_patch() {
        let content = r#"config/features=PackedStringArray("4.3.1", "Mobile")"#;
        let info = parse_project_info(content).unwrap();
        assert_eq!(info.version, GodotVersion::new(4, 3, 1));
        assert!(!info.mono);
    }

    #[test]
    fn parses_mono_without_renderer() {
        // Minimal project - just version and C#, no renderer feature.
        let content = r#"config/features=PackedStringArray("4.2", "C#")"#;
        let info = parse_project_info(content).unwrap();
        assert_eq!(info.version, GodotVersion::new(4, 2, 0));
        assert!(info.mono);
    }

    #[test]
    fn returns_none_when_no_features_line() {
        let content = r#"[application]

config/name="Old Project"
"#;
        assert!(parse_project_info(content).is_none());
    }

    #[test]
    fn returns_none_when_features_array_is_empty() {
        let content = r#"config/features=PackedStringArray()"#;
        assert!(parse_project_info(content).is_none());
    }
}
