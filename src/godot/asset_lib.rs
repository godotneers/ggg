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
use serde::{Deserialize, Deserializer};

const API_BASE: &str = "https://godotengine.org/asset-library/api";

// ---------------------------------------------------------------------------
// Wire format helpers
// ---------------------------------------------------------------------------

/// Deserialise a field that the API returns as a JSON string containing a
/// decimal integer (e.g. `"1586"`) into a `u32`.
fn de_string_u32<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let s = String::deserialize(d)?;
    s.parse::<u32>().map_err(serde::de::Error::custom)
}

/// Deserialise `download_hash`: treat an empty string as `None`.
fn de_optional_hash<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let s = String::deserialize(d)?;
    Ok(if s.is_empty() { None } else { Some(s) })
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
/// the given Godot `major.minor` version string (e.g. `"4.3"`).
///
/// Returns `(results, total_count)`.  `total_count` may be larger than
/// `results.len()` when there are multiple pages.
pub fn search(query: &str, godot_version: &str) -> Result<(Vec<AssetSearchResult>, u32)> {
    let client = build_client()?;
    let url = format!("{API_BASE}/asset");
    let mut req = client
        .get(&url)
        .query(&[
            ("filter", query),
            ("support", "official+community"),
            ("sort", "updated"),
        ]);
    if !godot_version.is_empty() {
        req = req.query(&[("godot_version", godot_version)]);
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
    let url = format!("{API_BASE}/asset/{id}");
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
