//! Client for the Godot Asset Library HTTP API.
//!
//! Base URL: `https://godotengine.org/asset-library/api`
//!
//! Two endpoints are used:
//! - `GET /asset?filter=<q>&godot_version=<X.Y>&support=official+community`
//! - `GET /asset/<id>` - full detail for one asset
//!
//! The API only exposes the **current** version of each asset; there is no
//! version history endpoint.
//!
//! # Wire format quirks
//!
//! Several numeric fields (`asset_id`, `version`) are returned as JSON strings
//! rather than numbers, and `download_hash` may be an empty string when the
//! asset author has not provided one.  The deserializers below handle these.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::godot::release::GodotVersion;
use crate::utils::de::{de_optional_hash, de_string_u32};

/// The Asset Library API base URL, overridable via the `GGG_ASSET_LIB_API_URL`
/// environment variable ([`crate::envvars::ASSET_LIB_API_URL_ENV_VAR`]) so
/// tests can point at a local server.
pub fn asset_lib_api_url() -> String {
    std::env::var(crate::envvars::ASSET_LIB_API_URL_ENV_VAR)
        .unwrap_or_else(|_| "https://godotengine.org/asset-library/api".to_string())
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// One entry in a search result page.
#[derive(Debug, Deserialize)]
pub struct AssetSearchResult {
    #[serde(deserialize_with = "de_string_u32")]
    pub asset_id: u32,
    pub title: String,
    pub author: String,
    /// SPDX license identifier, e.g. `"MIT"`.
    #[serde(rename = "cost")]
    pub license: String,
}

/// Full detail for a single asset, returned by `/asset/<id>`.
#[derive(Debug, Deserialize)]
pub struct AssetDetail {
    #[serde(deserialize_with = "de_string_u32")]
    pub asset_id: u32,
    pub title: String,
    pub author: String,
    /// SPDX license identifier, e.g. `"MIT"`.
    #[serde(rename = "cost")]
    pub license: String,
    /// Monotonically increasing integer version counter.
    #[serde(deserialize_with = "de_string_u32")]
    pub version: u32,
    /// Human-readable version string, e.g. `"9.3.0"`.
    pub version_string: String,
    /// Direct URL to the current archive (`.zip`).
    pub download_url: String,
    /// SHA-256 hex digest of the archive at `download_url`, or `None` when
    /// the asset author has not provided one.
    #[serde(deserialize_with = "de_optional_hash")]
    pub download_hash: Option<String>,
    /// URL for the asset's page on the asset library website.
    pub browse_url: String,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(rename = "total_items")]
    total: u32,
    result: Vec<AssetSearchResult>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Search the asset library for `query`, filtered to assets compatible with
/// the given Godot version (the `godot_version` API filter, sent as
/// `MAJOR.MINOR`).  Pass `None` to skip the filter.
///
/// Returns `(results, total_count)`.  `total_count` may be larger than
/// `results.len()` when there are multiple pages.
pub fn search(
    query: &str,
    godot_version: Option<&GodotVersion>,
) -> Result<(Vec<AssetSearchResult>, u32)> {
    let client = build_client()?;
    let url = format!("{}/asset", asset_lib_api_url());
    let mut req = client.get(&url).query(&[
        ("filter", query),
        ("support", "official+community"),
        ("sort", "updated"),
    ]);
    if let Some(version) = godot_version {
        req = req.query(&[("godot_version", version.major_minor())]);
    }
    let response: SearchResponse = req
        .send()
        .with_context(|| format!("failed to reach asset library API at {url}"))?
        .error_for_status()
        .context("asset library API returned an error")?
        .json()
        .context("failed to parse asset library search response")?;

    Ok((response.result, response.total))
}

/// Fetch full details for the asset with the given `id`.
pub fn get_asset(id: u32) -> Result<AssetDetail> {
    let client = build_client()?;
    let url = format!("{}/asset/{id}", asset_lib_api_url());
    client
        .get(&url)
        .send()
        .with_context(|| format!("failed to reach asset library API at {url}"))?
        .error_for_status()
        .with_context(|| format!("asset library returned an error for asset id {id}"))?
        .json()
        .context("failed to parse asset library detail response")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .build()
        .context("failed to build HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn asset_lib_api_url_override_wins() {
        unsafe {
            std::env::set_var(
                crate::envvars::ASSET_LIB_API_URL_ENV_VAR,
                "http://localhost:8080/api",
            )
        };
        assert_eq!(asset_lib_api_url(), "http://localhost:8080/api");
        unsafe { std::env::remove_var(crate::envvars::ASSET_LIB_API_URL_ENV_VAR) };
    }

    #[test]
    #[serial]
    fn asset_lib_api_url_default_when_unset() {
        unsafe { std::env::remove_var(crate::envvars::ASSET_LIB_API_URL_ENV_VAR) };
        assert_eq!(
            asset_lib_api_url(),
            "https://godotengine.org/asset-library/api"
        );
    }
}
