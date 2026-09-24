//! Central definitions for every environment variable ggg reads.
//!
//! Keeping each variable name in one place means production code, tests, and
//! documentation stay in sync: introducing or renaming a variable is a
//! single-line change instead of a hunt through string literals.

/// Overrides the root directory for the shared cache
/// ([`crate::cache::resolve_cache_root`]).
pub const CACHE_DIR_ENV_VAR: &str = "GGG_CACHE_DIR";

/// Overrides the Godot versions manifest URL
/// ([`crate::godot::manifest::versions_manifest_url`]).
pub const GODOT_MANIFEST_URL_ENV_VAR: &str = "GGG_GODOT_MANIFEST_URL";

/// Overrides the Godot Asset Library API base URL
/// ([`crate::godot::asset_lib::asset_lib_api_url`]).
pub const ASSET_LIB_API_URL_ENV_VAR: &str = "GGG_ASSET_LIB_API_URL";

/// Overrides the Godot Asset Store API base URL
/// ([`crate::godot::asset_store::asset_store_api_url`]).
pub const ASSET_STORE_API_URL_ENV_VAR: &str = "GGG_ASSET_STORE_API_URL";

/// Overrides the Godot builds (GitHub releases) API base URL
/// ([`crate::godot::download::godot_builds_api_url`]).
pub const GODOT_BUILDS_API_URL_ENV_VAR: &str = "GGG_GODOT_BUILDS_API_URL";

/// Overrides the Godot downloads base URL used for export templates
/// ([`crate::godot::export_templates`]).
pub const GODOT_DOWNLOADS_BASE_URL_ENV_VAR: &str = "GGG_GODOT_DOWNLOADS_BASE_URL";

/// Overrides the Godot editor data directory
/// ([`crate::godot::export_templates`]).
pub const GODOT_DATA_DIR_ENV_VAR: &str = "GGG_GODOT_DATA_DIR";

/// When set (to any value), suppresses coloured output from `ggg diff`.
pub const NO_COLOR_ENV_VAR: &str = "NO_COLOR";
