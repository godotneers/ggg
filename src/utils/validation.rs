//! Shared input validation helpers.
//!
//! The ggg codebase validates several kinds of user-provided strings: Godot
//! Asset Store slugs and version tags, archive URL extensions, and filesystem
//! path components. Keeping them here means configuration parsing, the CLI,
//! and the engine release handling use identical rules.

use anyhow::Result;

/// Returns an error unless `value` is a 3-256 character
/// lowercase-alphanumeric-and-`-` slug.
///
/// Matches the Asset Store frontend's own `url_slug` field constraints
/// (`pattern="[a-z0-9\-]+" minlength="3" maxlength="256"`). Exported so the
/// CLI can recognise a store spec (`publisher/asset[:version]`) before a
/// version has been resolved.
pub(crate) fn validate_asset_store_slug(field: &str, value: &str) -> Result<()> {
    if (3..=256).contains(&value.chars().count())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        Ok(())
    } else {
        anyhow::bail!(
            "{field} slug {value:?} must be 3-256 characters and contain only lowercase letters, digits, and '-'"
        )
    }
}

/// Returns an error unless `value` follows docker-tag syntax: starts with an
/// alphanumeric character, followed by alphanumerics, `.`, `_`, or `-`.
///
/// Exported for the same reason as [`validate_asset_store_slug`]: the CLI
/// validates a `:version` suffix without needing an `AssetStoreRef`.
pub(crate) fn validate_version_tag(value: &str) -> Result<()> {
    let mut chars = value.chars();
    let valid = match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        anyhow::bail!(
            "version {value:?} must start with a letter or digit and contain only \
             letters, digits, '.', '_', and '-'"
        )
    }
}

/// Returns an error unless `url` ends in a supported archive extension.
pub(crate) fn validate_archive_url(url: &str) -> Result<()> {
    let supported = url.ends_with(".zip") || url.ends_with(".tar.gz") || url.ends_with(".tgz");
    if supported {
        Ok(())
    } else {
        anyhow::bail!(
            "unrecognised archive format in URL {url:?}; supported extensions: .zip, .tar.gz, .tgz"
        )
    }
}

/// Returns an error if `value` contains characters that could escape a
/// directory when used as a path component.
///
/// Allowed: alphanumeric characters, `.`, `-`. Everything else is rejected,
/// including `/`, `\`, and `..` sequences.
pub(crate) fn validate_path_component(field: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        anyhow::bail!("GodotRelease {field} must not be empty");
    }
    if !value
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
    {
        anyhow::bail!("GodotRelease {field} contains invalid characters: \"{value}\"");
    }
    Ok(())
}
