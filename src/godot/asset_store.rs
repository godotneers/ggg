//! Client for the Godot Asset Store HTTP API.
//!
//! Base URL: `https://store.godotengine.org/api/v1`
//!
//! Three endpoints are used (all public, no authentication required):
//! - `GET /search/query/` - search addons (`type=0`)
//! - `GET /assets/{publisher}/{asset}/` - full detail for one asset
//! - `GET /releases/{publisher}/{asset}/` - version history for one asset
//!
//! # Wire format quirks
//!
//! The upstream OpenAPI spec is partly inaccurate, so only the fields the
//! dependency pipeline needs are deserialized and everything else is ignored:
//! - `SearchResults.count` is a JSON string (`"427"`) rather than a number.
//! - `ReleaseData.max_godot_version` is nullable.
//!
//! The `compatibility` query parameter takes a Godot version series, which
//! this client derives from a [`GodotVersion`] as `MAJOR.MINOR` (e.g. `"4.3"`).
//! Full semantic versions are also accepted by the server, but suffixed
//! versions such as `"4.3-stable"` are rejected with a 422, so the client
//! never sends patch or flavor suffixes.

use std::borrow::Cow;
use std::cmp::Ordering;

use anyhow::{Context, Result};
use serde::Deserialize;
use versions::Versioning;

use crate::godot::release::GodotVersion;
use crate::utils::de::de_string_u32;

/// The Asset Store API base URL, overridable via the
/// `GGG_ASSET_STORE_API_URL` environment variable
/// ([`crate::envvars::ASSET_STORE_API_URL_ENV_VAR`]) so tests can point at a
/// local server.
pub fn asset_store_api_url() -> String {
    std::env::var(crate::envvars::ASSET_STORE_API_URL_ENV_VAR)
        .unwrap_or_else(|_| "https://store.godotengine.org/api/v1".to_string())
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Publisher metadata as embedded in an asset response.
#[derive(Debug, Deserialize)]
pub struct StorePublisher {
    /// Unique slug, e.g. `"souleat"`.
    pub slug: String,
    /// Human-readable display name.
    pub name: String,
}

/// An asset in the Asset Store, as returned by both the search endpoint and
/// the asset detail endpoint.
#[derive(Debug, Deserialize)]
pub struct StoreAsset {
    /// Unique asset slug, e.g. `"godot-xoshiro256-plus-plus"`.
    pub slug: String,
    pub publisher: StorePublisher,
    /// Human-readable display name, e.g. `"GodotXoshiro256++"`.
    pub name: String,
    /// SPDX license identifier, e.g. `"MIT"`.
    #[serde(rename = "license_type")]
    pub license: String,
    /// URL for the asset's page on the store website.
    pub store_url: String,
}

/// One release of an Asset Store asset, from `GET /releases/{publisher}/{asset}/`.
#[derive(Debug, Clone, Deserialize)]
pub struct StoreRelease {
    /// Numeric release ID (unique; `version` is only a display name).
    pub id: u64,
    /// Human-readable version string, e.g. `"1.0.0"`.
    pub version: String,
    /// Whether this is a stable release of the asset.
    pub stable: bool,
    /// Direct URL to the downloadable archive (a presigned URL that expires).
    pub download_url: String,
    /// Minimum compatible Godot version, e.g. `"4.0"`.
    pub min_godot_version: String,
    /// Maximum compatible Godot version, or `None` when unbounded.
    pub max_godot_version: Option<String>,
}

/// One entry within a search page's `hits` array. The `asset` object carries
/// all fields the pipeline needs; `highlights` and `tag_filters` are ignored.
#[derive(Deserialize)]
struct SearchHit {
    asset: StoreAsset,
}

#[derive(Deserialize)]
struct SearchResponse {
    /// Total number of matches; a string per the API (`"427"`), even though
    /// it is semantically a count.
    #[serde(deserialize_with = "de_string_u32")]
    count: u32,
    hits: Vec<SearchHit>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Search the Asset Store for `query`, filtered to addons (`type=0`)
/// compatible with the given Godot version (the `compatibility` API filter,
/// sent as `MAJOR.MINOR`). Pass `None` to skip the filter.
///
/// Returns `(results, total_count)`.  Only the first page of results is
/// returned; `total_count` may be larger than `results.len()` when there are
/// more matches (the response carries a `scroll` token for fetching them).
pub fn search(query: &str, godot_version: Option<&GodotVersion>) -> Result<(Vec<StoreAsset>, u32)> {
    let client = build_client()?;
    let url = format!("{}/search/query/", asset_store_api_url());
    let mut req = client.get(&url).query(&[("type", "0"), ("query", query)]);
    if let Some(version) = godot_version {
        req = req.query(&[("compatibility", version.major_minor())]);
    }
    let response: SearchResponse = req
        .send()
        .with_context(|| format!("failed to reach Asset Store API at {url}"))?
        .error_for_status()
        .context("Asset Store API returned an error")?
        .json()
        .context("failed to parse Asset Store search response")?;

    let results = response.hits.into_iter().map(|hit| hit.asset).collect();
    Ok((results, response.count))
}

/// Fetch details for the asset identified by `publisher` and asset `slug`.
pub fn get_asset(publisher: &str, slug: &str) -> Result<StoreAsset> {
    let client = build_client()?;
    let url = format!("{}/assets/{publisher}/{slug}/", asset_store_api_url());
    client
        .get(&url)
        .send()
        .with_context(|| format!("failed to reach Asset Store API at {url}"))?
        .error_for_status()
        .with_context(|| format!("Asset Store returned an error for asset {publisher}/{slug}"))?
        .json()
        .context("failed to parse Asset Store asset response")
}

/// List the releases of the asset identified by `publisher` and asset `slug`.
///
/// `godot_version` is sent to the API as the `compatibility` filter (when
/// `Some`) and `stable_only` is forwarded verbatim when set; the server does
/// the filtering.
pub fn get_releases(
    publisher: &str,
    slug: &str,
    godot_version: Option<&GodotVersion>,
    stable_only: bool,
) -> Result<Vec<StoreRelease>> {
    let client = build_client()?;
    let url = format!("{}/releases/{publisher}/{slug}/", asset_store_api_url());
    let mut req = client.get(&url);
    if stable_only {
        req = req.query(&[("stable_only", "true")]);
    }
    if let Some(version) = godot_version {
        req = req.query(&[("compatibility", version.major_minor())]);
    }
    req.send()
        .with_context(|| format!("failed to reach Asset Store API at {url}"))?
        .error_for_status()
        .with_context(|| {
            format!("Asset Store returned an error for releases of {publisher}/{slug}")
        })?
        .json()
        .context("failed to parse Asset Store releases response")
}

// ---------------------------------------------------------------------------
// Release selection
// ---------------------------------------------------------------------------

/// Select the release whose version string is `version`, preferring the larger
/// release id when several releases share the same version string.
///
/// Pure (no network), so it is unit-tested directly. The caller is expected
/// to have fetched the full, unfiltered release list via `get_releases`.
pub(crate) fn select_pinned_release<'a>(
    releases: &'a [StoreRelease],
    version: &str,
) -> Option<&'a StoreRelease> {
    releases
        .iter()
        .filter(|r| r.version == version)
        .max_by_key(|r| r.id)
}

/// Select the latest stable release compatible with `godot_version`.
///
/// "Latest" is decided by semantic version comparison of the release version
/// strings, with the larger numeric release id breaking ties when several
/// releases share the same version string. Ordering by version first (rather
/// than by release id) means a later-uploaded patch of an old series - e.g.
/// `3.9.2` with id 121 published after `5.0.0` with id 120 - never outranks
/// the genuinely higher version.
///
/// Pure (no network), so it is unit-tested directly. The caller is expected
/// to have fetched the full, unfiltered release list via `get_releases`.
/// Shared by `ggg add` (bare-spec selection) and `ggg update` so the two
/// commands agree on what "newest" means.
pub(crate) fn select_latest_compatible<'a>(
    releases: &'a [StoreRelease],
    godot_version: &GodotVersion,
) -> Option<&'a StoreRelease> {
    releases
        .iter()
        .filter(|r| r.stable && r.is_compatible_with(godot_version))
        .max_by(|a, b| cmp_store_versions(&a.version, &b.version).then_with(|| a.id.cmp(&b.id)))
}

impl StoreRelease {
    /// Whether this release's Godot compatibility range covers `version`.
    ///
    /// The min bound is inclusive; a `None` max means unbounded above. Bounds
    /// that do not parse as a version (the store may send free-form strings)
    /// are treated as unbounded so ggg never blocks an install on a
    /// misinterpreted value.
    pub(crate) fn is_compatible_with(&self, version: &GodotVersion) -> bool {
        let min_ok = parse_tolerant(&self.min_godot_version)
            .map(|min| version >= &min)
            .unwrap_or(true);
        let max_ok = self
            .max_godot_version
            .as_deref()
            .and_then(parse_tolerant)
            .map(|max| version <= &max)
            .unwrap_or(true);
        min_ok && max_ok
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .build()
        .context("failed to build HTTP client")
}

/// Compare two Asset Store version strings semantically.
///
/// Each string is first normalised via [`normalize_store_version`], then
/// parsed with the `versions` crate, which understands strict SemVer
/// (`1.2.3`), looser general versions (`1.27.23.124`, `12.4-beta`), and
/// arbitrary forms. A string that cannot be parsed at all is treated as lower
/// than any parseable one; when neither side parses, plain lexical comparison
/// decides. The result is a total order, so it can safely drive
/// "which release is newest" selection.
pub(crate) fn cmp_store_versions(a: &str, b: &str) -> Ordering {
    let a = normalize_store_version(a);
    let b = normalize_store_version(b);
    match (Versioning::new(&a), Versioning::new(&b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => a.cmp(&b),
    }
}

/// Normalise a store version string before comparison: trim surrounding
/// whitespace, drop a single leading `v`/`V`, and pad bare two-component
/// versions to three components (see [`pad_two_part`]).
fn normalize_store_version(s: &str) -> Cow<'_, str> {
    let trimmed = s.trim();
    let stripped = trimmed.strip_prefix(['v', 'V']).unwrap_or(trimmed);
    pad_two_part(stripped)
}

/// Rewrite a bare `major.minor` version to `major.minor.0`.
///
/// The `versions` crate's *general* scheme reads a `-suffix` on a two-part
/// version as a packaging revision, so `12.4-beta` would outrank `12.4` -
/// the opposite of the usual pre-release semantics. Padding the version to
/// three components makes it parse as strict SemVer, where a pre-release
/// correctly sorts below its release. Versions with three or more components,
/// or with a letter in the second slot (e.g. `8.u51-1`), are left untouched.
fn pad_two_part(s: &str) -> Cow<'_, str> {
    let Some((major, rest)) = s.split_once('.') else {
        return Cow::Borrowed(s);
    };
    if major.is_empty() || !major.bytes().all(|b| b.is_ascii_digit()) {
        return Cow::Borrowed(s);
    }
    let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return Cow::Borrowed(s);
    }
    if digits == rest.len() {
        return Cow::Owned(format!("{major}.{rest}.0"));
    }
    if rest.as_bytes()[digits] == b'-' {
        let (minor, suffix) = rest.split_at(digits);
        return Cow::Owned(format!("{major}.{minor}.0{suffix}"));
    }
    Cow::Borrowed(s)
}

/// Parse a Godot version that may be missing components (e.g. `"4"`), which
/// [`GodotVersion`]'s own `FromStr` rejects. Returns `None` when unparseable.
fn parse_tolerant(s: &str) -> Option<GodotVersion> {
    let parts: Vec<u32> = s
        .trim()
        .split('.')
        .map(|p| p.parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    match parts.as_slice() {
        [major] => Some(GodotVersion::new(*major, 0, 0)),
        [major, minor] => Some(GodotVersion::new(*major, *minor, 0)),
        [major, minor, patch] => Some(GodotVersion::new(*major, *minor, *patch)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn asset_store_api_url_override_wins() {
        unsafe {
            std::env::set_var(
                crate::envvars::ASSET_STORE_API_URL_ENV_VAR,
                "http://localhost:8080/api/v1",
            )
        };
        assert_eq!(asset_store_api_url(), "http://localhost:8080/api/v1");
        unsafe { std::env::remove_var(crate::envvars::ASSET_STORE_API_URL_ENV_VAR) };
    }

    #[test]
    #[serial]
    fn asset_store_api_url_default_when_unset() {
        unsafe { std::env::remove_var(crate::envvars::ASSET_STORE_API_URL_ENV_VAR) };
        assert_eq!(
            asset_store_api_url(),
            "https://store.godotengine.org/api/v1"
        );
    }

    // --- Release selection ------------------------------------------------

    fn store_release(id: u64, version: &str, min: &str, max: Option<&str>) -> StoreRelease {
        StoreRelease {
            id,
            version: version.to_string(),
            stable: true,
            download_url: format!("https://example.com/releases/{id}.zip"),
            min_godot_version: min.to_string(),
            max_godot_version: max.map(str::to_string),
        }
    }

    #[test]
    fn select_pinned_release_matches_version_string() {
        let releases = [
            store_release(1, "1.0.0", "4.0", Some("4.4")),
            store_release(2, "1.1.0", "4.0", None),
        ];
        let selected = select_pinned_release(&releases, "1.0.0").unwrap();
        assert_eq!(selected.id, 1);
    }

    #[test]
    fn select_pinned_release_duplicate_version_picks_larger_id() {
        // Two releases share the version string "1.1.0"; the larger release id
        // is the "real" latest entry and must win.
        let releases = [
            store_release(10, "1.1.0", "4.0", None),
            store_release(7, "1.1.0", "4.0", None),
            store_release(11, "1.2.0", "4.0", None),
        ];
        let selected = select_pinned_release(&releases, "1.1.0").unwrap();
        assert_eq!(selected.id, 10);
    }

    #[test]
    fn select_pinned_release_returns_none_for_unknown_and_empty_lists() {
        let releases = [store_release(1, "1.0.0", "4.0", None)];
        assert!(select_pinned_release(&releases, "9.9.9").is_none());
        assert!(select_pinned_release(&[], "1.0.0").is_none());
    }

    #[test]
    fn normalize_store_version_trims_and_strips_v_prefix() {
        assert_eq!(normalize_store_version("1.2.3"), "1.2.3");
        assert_eq!(normalize_store_version("v1.2.3"), "1.2.3");
        assert_eq!(normalize_store_version("V1.2.3"), "1.2.3");
        assert_eq!(normalize_store_version(" 1.2.3 "), "1.2.3");
        // Only the leading prefix of the trimmed string is stripped.
        assert_eq!(normalize_store_version("1.2.3-v2"), "1.2.3-v2");
        // Three-or-more-part versions, and second-slot letters, are unchanged.
        assert_eq!(normalize_store_version("1.27.23.124"), "1.27.23.124");
        assert_eq!(normalize_store_version("8.u51-1"), "8.u51-1");
    }

    #[test]
    fn normalize_store_version_pads_two_part_versions() {
        // Bare two-part versions are padded so their `-suffix` reads as a
        // SemVer pre-release rather than a packaging revision.
        assert_eq!(normalize_store_version("12.4"), "12.4.0");
        assert_eq!(normalize_store_version("12.4-beta"), "12.4.0-beta");
        assert_eq!(normalize_store_version("v12.4-alpha.2"), "12.4.0-alpha.2");
    }

    #[test]
    fn cmp_store_versions_semver_ordering() {
        assert_eq!(cmp_store_versions("5.0.0", "3.9.2"), Ordering::Greater);
        assert_eq!(cmp_store_versions("3.9.2", "5.0.0"), Ordering::Less);
        assert_eq!(cmp_store_versions("4.0.4", "4.0.4"), Ordering::Equal);
        assert_eq!(cmp_store_versions("2.1.10", "2.1.9"), Ordering::Greater);
    }

    #[test]
    fn cmp_store_versions_ignores_leading_v_and_whitespace() {
        assert_eq!(cmp_store_versions("v1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(cmp_store_versions("V1.2.3", "v1.2.3"), Ordering::Equal);
        assert_eq!(cmp_store_versions(" 1.2.3 ", "1.2.3"), Ordering::Equal);
        assert_eq!(cmp_store_versions("v2.0.0", "1.9.9"), Ordering::Greater);
    }

    #[test]
    fn cmp_store_versions_supports_four_part_versions() {
        // Four numeric parts are not strict SemVer but must still compare.
        assert_eq!(
            cmp_store_versions("1.27.23.124", "1.27.23.123"),
            Ordering::Greater
        );
        assert_eq!(cmp_store_versions("1.27.23.124", "1.27.24"), Ordering::Less);
        assert_eq!(cmp_store_versions("8.64.0.81", "8.65.0.78"), Ordering::Less);
    }

    #[test]
    fn cmp_store_versions_treats_prereleases_as_lower() {
        // A pre-release sorts below its own release but above earlier
        // pre-releases.
        assert_eq!(cmp_store_versions("12.4-beta", "12.4"), Ordering::Less);
        assert_eq!(cmp_store_versions("12.4", "12.4-beta"), Ordering::Greater);
        assert_eq!(
            cmp_store_versions("12.4-beta", "12.4-alpha"),
            Ordering::Greater
        );
        assert_eq!(cmp_store_versions("1.0.0-rc.1", "1.0.0"), Ordering::Less);
    }

    #[test]
    fn cmp_store_versions_parseable_outranks_unparseable() {
        assert_eq!(
            cmp_store_versions("1.2.3", "not a version"),
            Ordering::Greater
        );
        assert_eq!(cmp_store_versions("not a version", "1.2.3"), Ordering::Less);
    }

    #[test]
    fn cmp_store_versions_unparseable_falls_back_to_lexical() {
        assert_eq!(cmp_store_versions("alpha-x", "alpha-y"), Ordering::Less);
        assert_eq!(cmp_store_versions("zulu", "alpha"), Ordering::Greater);
        assert_eq!(cmp_store_versions("alpha", "alpha"), Ordering::Equal);
    }

    #[test]
    fn select_latest_compatible_prefers_higher_version_over_higher_id() {
        // The maintainer released 5.0.0 (id 120), then back-patched the 3.x
        // series as 3.9.2 (id 121). Ordering by id alone would pick 3.9.2 and
        // downgrade a 4.0.4 user; version-first ordering must pick 5.0.0.
        let releases = [
            store_release(119, "4.0.4", "4.0", None),
            store_release(120, "5.0.0", "4.0", None),
            store_release(121, "3.9.2", "4.0", None),
        ];
        let selected = select_latest_compatible(&releases, &GodotVersion::new(4, 0, 0)).unwrap();
        assert_eq!(selected.version, "5.0.0");
        assert_eq!(selected.id, 120);
    }

    #[test]
    fn select_latest_compatible_breaks_version_ties_by_higher_id() {
        let releases = [
            store_release(7, "1.1.0", "4.0", None),
            store_release(10, "1.1.0", "4.0", None),
            store_release(11, "1.2.0", "4.0", None),
        ];
        let selected = select_latest_compatible(&releases, &GodotVersion::new(4, 0, 0)).unwrap();
        assert_eq!(selected.version, "1.2.0");
        assert_eq!(selected.id, 11);
    }

    #[test]
    fn select_latest_compatible_skips_unstable_and_incompatible() {
        let unstable = StoreRelease {
            id: 99,
            version: "2.0.0-beta".to_string(),
            stable: false,
            download_url: "https://example.com/releases/99.zip".to_string(),
            min_godot_version: "4.0".to_string(),
            max_godot_version: None,
        };
        let releases = [
            store_release(1, "1.0.0", "4.0", Some("4.4")),
            unstable,
            store_release(98, "3.0.0", "5.0", None), // requires Godot 5
            store_release(97, "1.1.0", "4.0", None),
        ];
        let selected = select_latest_compatible(&releases, &GodotVersion::new(4, 2, 0)).unwrap();
        assert_eq!(selected.version, "1.1.0");
        assert_eq!(selected.id, 97);
    }

    #[test]
    fn select_latest_compatible_returns_none_for_no_compatible() {
        let releases = [store_release(1, "1.0.0", "5.0", None)];
        assert!(select_latest_compatible(&releases, &GodotVersion::new(4, 2, 0)).is_none());
        assert!(select_latest_compatible(&[], &GodotVersion::new(4, 2, 0)).is_none());
    }

    #[test]
    fn is_compatible_respects_min_and_max_bounds_inclusive() {
        let release = store_release(1, "1.0.0", "4.0", Some("4.4"));
        assert!(release.is_compatible_with(&GodotVersion::new(4, 0, 0)));
        assert!(release.is_compatible_with(&GodotVersion::new(4, 4, 0)));
        assert!(release.is_compatible_with(&GodotVersion::new(4, 2, 1)));
        assert!(!release.is_compatible_with(&GodotVersion::new(3, 5, 0)));
        assert!(!release.is_compatible_with(&GodotVersion::new(4, 5, 0)));

        let unbounded = store_release(2, "1.1.0", "4.2", None);
        assert!(unbounded.is_compatible_with(&GodotVersion::new(4, 9, 9)));
        assert!(!unbounded.is_compatible_with(&GodotVersion::new(4, 1, 0)));
    }

    #[test]
    fn is_compatible_treats_unparseable_bounds_as_unbounded() {
        // Free-form bounds (suffixes, garbage) must never block an install -
        // they are treated as "no constraint".
        let freeform = store_release(1, "1.0.0", "4.x", Some("stable"));
        assert!(freeform.is_compatible_with(&GodotVersion::new(3, 0, 0)));
        assert!(freeform.is_compatible_with(&GodotVersion::new(9, 9, 9)));
    }

    #[test]
    fn is_compatible_handles_single_component_min() {
        // "4" pins the major series via tolerant parsing, so older majors are
        // genuinely incompatible while anything in 4.x is fine.
        let release = store_release(1, "1.0.0", "4", None);
        assert!(!release.is_compatible_with(&GodotVersion::new(3, 5, 0)));
        assert!(release.is_compatible_with(&GodotVersion::new(4, 0, 0)));
        assert!(release.is_compatible_with(&GodotVersion::new(4, 9, 9)));
    }

    #[test]
    fn parse_tolerant_accepts_missing_components() {
        assert_eq!(parse_tolerant("4"), Some(GodotVersion::new(4, 0, 0)));
        assert_eq!(parse_tolerant("4.3"), Some(GodotVersion::new(4, 3, 0)));
        assert_eq!(parse_tolerant("4.3.1"), Some(GodotVersion::new(4, 3, 1)));
        assert_eq!(parse_tolerant("4.3 "), Some(GodotVersion::new(4, 3, 0)));
        assert_eq!(parse_tolerant(""), None);
        assert_eq!(parse_tolerant("4.x"), None);
        assert_eq!(parse_tolerant("4.3.0.1"), None);
    }
}
