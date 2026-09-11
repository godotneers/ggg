//! A wiremock-backed fake of the remote services ggg talks to.
//!
//! ggg only reaches the network through a handful of endpoints (the Godot
//! versions manifest, the Asset Library API, the GitHub builds API, and the
//! official downloads host). This [`MockApi`] starts a local wiremock server
//! and mounts stubs for those endpoints so integration tests need no real
//! network.
//!
//! Every stub response embeds the live server port at runtime (via
//! [`MockServer::uri`]), which is what makes the round-trips hermetic:
//! the "download URLs" the API reports point back at the same mock server.

use serde::Serialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::archive::fake_godot_asset;
use ggg::godot::release::GodotRelease;

/// Serialise a `u32` as a decimal JSON string, matching how the Asset Library
/// API returns numeric fields (e.g. `"1586"` instead of `1586`).
fn serialize_u32_as_string<S: serde::Serializer>(v: &u32, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}

/// Serialise an optional archive hash as a JSON string: `None` becomes the
/// empty string, matching the API's "author did not provide a hash".
fn serialize_optional_hash<S: serde::Serializer>(
    v: &Option<String>,
    s: S,
) -> Result<S::Ok, S::Error> {
    s.serialize_str(v.as_deref().unwrap_or(""))
}

/// One entry in an Asset Library search result page.
#[derive(Debug, Serialize)]
pub struct AssetSummary {
    #[serde(serialize_with = "serialize_u32_as_string")]
    pub asset_id: u32,
    pub title: String,
    pub author: String,
    /// SPDX license identifier, e.g. `"MIT"`.
    pub cost: String,
}

impl AssetSummary {
    /// Fill a summary from string literals without `.into()` on every field.
    pub fn new(
        asset_id: u32,
        title: impl Into<String>,
        author: impl Into<String>,
        cost: impl Into<String>,
    ) -> Self {
        Self {
            asset_id,
            title: title.into(),
            author: author.into(),
            cost: cost.into(),
        }
    }
}

/// A search page response for `GET /asset`.
#[derive(Debug, Serialize)]
pub struct AssetSearchBody {
    pub total_items: u32,
    pub result: Vec<AssetSummary>,
}

impl AssetSearchBody {
    pub fn new(total_items: u32, result: Vec<AssetSummary>) -> Self {
        Self {
            total_items,
            result,
        }
    }
}

/// A full-detail response for `GET /asset/{id}`.
///
/// Fields mirror the real API; optional fields are `Option` so tests only
/// supply what they need. URL fields may contain a `{base}` placeholder that
/// is replaced with the live server port at mount time.
///
/// There are two mutually exclusive shapes, one per constructor:
/// [`Self::new`] reports a `download_url` supplied by the caller, while
/// [`Self::new_with_file`] hands the archive to
/// [`MockApi::mount_asset_detail`], which serves those bytes and rewrites
/// `download_url` — so a file and a URL never have to be kept in sync.
#[derive(Debug, Serialize)]
pub struct AssetDetailBody {
    #[serde(serialize_with = "serialize_u32_as_string")]
    pub asset_id: u32,
    pub title: String,
    pub author: String,
    /// SPDX license identifier, e.g. `"MIT"`.
    pub cost: String,
    /// Monotonically increasing integer version counter.
    #[serde(serialize_with = "serialize_u32_as_string")]
    pub version: u32,
    /// Human-readable version string, e.g. `"1.0.0"`.
    pub version_string: String,
    /// Direct URL to the current archive (`.zip`). Set by the caller via
    /// [`Self::new`]; for [`Self::new_with_file`] bodies,
    /// [`MockApi::mount_asset_detail`] owns this and rewrites it to a route
    /// serving the attached file.
    pub download_url: String,
    /// SHA-256 hex digest of the archive at `download_url`; `None` is
    /// serialised as the empty string.
    #[serde(serialize_with = "serialize_optional_hash")]
    pub download_hash: Option<String>,
    /// URL for the asset's page on the asset library website.
    pub browse_url: String,
    /// Optional archive (zip) served as `download_url`. Hidden from the
    /// serialised response — it is not part of the wire format.
    #[serde(skip)]
    pub file: Option<Vec<u8>>,
}

impl AssetDetailBody {
    /// Build the URL shape: the caller supplies `download_url` and
    /// `browse_url`, and no archive is served.
    ///
    /// All eight fields are required, so omitting one is a compile error. Use
    /// [`Self::new_with_file`] instead when the mock should serve the archive.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        asset_id: u32,
        title: impl Into<String>,
        author: impl Into<String>,
        cost: impl Into<String>,
        version: u32,
        version_string: impl Into<String>,
        download_url: impl Into<String>,
        browse_url: impl Into<String>,
    ) -> Self {
        Self {
            asset_id,
            title: title.into(),
            author: author.into(),
            cost: cost.into(),
            version,
            version_string: version_string.into(),
            download_url: download_url.into(),
            download_hash: None,
            browse_url: browse_url.into(),
            file: None,
        }
    }

    /// Build the file shape: attach the archive and omit `download_url`.
    ///
    /// [`MockApi::mount_asset_detail`] then serves these bytes and rewrites
    /// `download_url`, so the caller never supplies a URL that has to match a
    /// separately-mounted file. Only `browse_url` (and no `download_url`) is
    /// required on top of the shared fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_file(
        asset_id: u32,
        title: impl Into<String>,
        author: impl Into<String>,
        cost: impl Into<String>,
        version: u32,
        version_string: impl Into<String>,
        browse_url: impl Into<String>,
        file: Vec<u8>,
    ) -> Self {
        Self {
            asset_id,
            title: title.into(),
            author: author.into(),
            cost: cost.into(),
            version,
            version_string: version_string.into(),
            download_url: String::new(),
            download_hash: None,
            browse_url: browse_url.into(),
            file: Some(file),
        }
    }
}

/// One asset in a GitHub release, as reported by the builds API.
#[derive(Debug, Serialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

/// The GitHub `releases/tags/{tag}` API response body.
#[derive(Debug, Serialize)]
pub struct GodotReleaseBody {
    pub assets: Vec<ReleaseAsset>,
}

/// A running, hermetic fake of Godot's remote services backed by wiremock.
///
/// The server is bound to an ephemeral port on `127.0.0.1`. Tests point ggg at
/// it by setting the appropriate `GGG_*_URL` env var to [`Self::base_url`].
pub struct MockApi {
    server: MockServer,
}

impl MockApi {
    /// Start the server and return a [`MockApi`] ready to have stubs mounted.
    pub async fn start() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    /// The base URL of the running server, e.g. `http://127.0.0.1:49152`.
    ///
    /// Assign to a `GGG_*_URL` env var (e.g. `GGG_GODOT_MANIFEST_URL`) to point
    /// ggg at this fake.
    pub fn base_url(&self) -> String {
        self.server.uri()
    }

    /// Mount a stub for the Godot versions manifest endpoint.
    ///
    /// The YAML body is generated from the given `yaml` source with `{base}`
    /// replaced by the live server port. Returns a reference to `self` for
    /// chaining.
    pub async fn mount_manifest(&self, yaml: &str) -> &Self {
        let body = yaml.replace("{base}", &self.server.uri());
        Mock::given(method("GET"))
            .and(path("/manifest"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&self.server)
            .await;
        self
    }

    /// Mount a stub for the Asset Library search endpoint (`/asset`).
    ///
    /// The response is serialised from `body`, with any `{base}` placeholder
    /// replaced by the live server port at runtime. Returns a reference to
    /// `self` for chaining.
    pub async fn mount_asset_search(&self, body: &AssetSearchBody) -> &Self {
        let search = serde_json::to_string(body).expect("asset search body is serializable");
        let search = search.replace("{base}", &self.server.uri());

        Mock::given(method("GET"))
            .and(path("/asset"))
            .respond_with(ResponseTemplate::new(200).set_body_string(search))
            .mount(&self.server)
            .await;

        self
    }

    /// Mount a stub for the Asset Library detail endpoint (`/asset/{id}`).
    ///
    /// The stub only answers for the given `asset_id`, so several assets can
    /// each have their own detail response on the same server (e.g. remapping
    /// a dependency from one asset id to another).
    ///
    /// Two body shapes are supported, one per constructor, and neither can be
    /// misused:
    ///
    /// - [`AssetDetailBody::new`] with a plain `download_url` (a `{base}`
    ///   placeholder is allowed) is reported back verbatim;
    /// - [`AssetDetailBody::new_with_file`] lets the mock own the download URL:
    ///   it serves the bytes at a stable per-asset route
    ///   (`/files/asset-{id}.zip`), rewrites `download_url` to that route, and
    ///   mounts the file in one step.
    ///
    /// Returns a reference to `self` for chaining.
    pub async fn mount_asset_detail(&self, id: u32, mut body: AssetDetailBody) -> &Self {
        if let Some(file) = body.file.take() {
            let route = format!("/files/asset-{id}.zip");
            body.download_url = format!("{}{route}", self.server.uri());
            self.mount_file(&route, file).await;
        }

        let detail = serde_json::to_string(&body).expect("asset detail body is serializable");
        let detail = detail.replace("{base}", &self.server.uri());

        Mock::given(method("GET"))
            .and(path(format!("/asset/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(detail))
            .mount(&self.server)
            .await;

        self
    }

    /// Mount a stub that serves raw `bytes` at the given `path` (e.g. a zip
    /// archive at `/files/addon.zip`).
    ///
    /// This is how tests serve the actual downloadable artifact that archive
    /// and asset-library dependencies reference. Returns a reference to `self`
    /// for chaining.
    pub async fn mount_file(&self, route: &str, bytes: Vec<u8>) -> &Self {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(bytes, "application/octet-stream"),
            )
            .mount(&self.server)
            .await;
        self
    }

    /// Mount the GitHub builds API for `tag`: serves the `releases/tags/{tag}`
    /// JSON response and, for each `(name, bytes)` asset, a route that serves
    /// the archive bytes and a `browser_download_url` pointing at it.
    ///
    /// The whole engine-download round-trip therefore stays on the mock server:
    /// ggg queries `GET /releases/tags/{tag}`, finds the platform asset by
    /// name, then downloads it from the reported URL.
    pub async fn mount_builds_api(&self, tag: &str, assets: Vec<(String, Vec<u8>)>) -> &Self {
        let mut release_assets = Vec::new();
        for (i, (name, bytes)) in assets.into_iter().enumerate() {
            let route = format!("/files/godot-{tag}-{i}.zip");
            let browser_download_url = format!("{}{route}", self.server.uri());
            release_assets.push(ReleaseAsset {
                name,
                browser_download_url,
            });
            self.mount_file(&route, bytes).await;
        }

        let body = GodotReleaseBody {
            assets: release_assets,
        };
        let json = serde_json::to_string(&body).expect("builds API body is serializable");

        Mock::given(method("GET"))
            .and(path(format!("/releases/tags/{tag}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(json))
            .mount(&self.server)
            .await;

        self
    }

    /// Mount the GitHub builds API for `release` with a single fake engine
    /// archive for the current platform.
    ///
    /// Convenience wrapper around [`Self::mount_builds_api`] that builds the
    /// exact platform asset name (`fake_godot_asset`) so tests can express
    /// "this release is downloadable" in one call.
    pub async fn mount_godot_release(&self, release: &GodotRelease) -> &Self {
        let (name, bytes) = fake_godot_asset(release);
        self.mount_builds_api(&release.tag(), vec![(name, bytes)])
            .await
    }

    /// Mount the Godot downloads host's export-templates endpoint.
    ///
    /// ggg requests `/{?version,flavor,slug,platform}`; the mock answers with
    /// the given `.tpz` zip bytes so `ensure_export_templates` can download,
    /// cache, and install them.
    pub async fn mount_export_templates(&self, tpz: Vec<u8>) -> &Self {
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(tpz, "application/zip"))
            .mount(&self.server)
            .await;
        self
    }

    /// Drop every stub currently mounted on the server.
    ///
    /// wiremock resolves matching stubs in mount order (the first match wins),
    /// so re-stubbing an endpoint requires clearing the old stub first. This is
    /// how tests change an endpoint's response mid-scenario, e.g. simulating
    /// the asset library shipping a newer version between `ggg sync` and
    /// `ggg update`.
    pub async fn reset(&self) {
        self.server.reset().await;
    }

    /// Every request the server has handled so far, in arrival order.
    ///
    /// Lets tests assert on the actual request line a spawned `ggg` process
    /// sent, e.g. which query parameters were present (`godot_version`)
    /// without needing a stricter matcher. Each request's `url` is a parsed
    /// `url::Url`, so `query_pairs()` gives the raw query parameters.
    pub async fn received_requests(&self) -> Vec<wiremock::Request> {
        self.server.received_requests().await.unwrap_or_default()
    }
}
