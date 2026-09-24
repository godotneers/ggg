//! End-to-end tests for `ggg search`.
//!
//! These drive the compiled `ggg` binary against wiremock-backed fakes of the
//! Godot Asset Library and Asset Store APIs (see `common::wiremock`) and
//! assert on the rendered table, the version filter pulled from `ggg.toml` /
//! `--godot-version`, and the pagination/truncation message.
//!
//! The default source is the Asset Store; the asset-library source must be
//! requested explicitly with `--source asset-library`.

mod common;

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

use common::TestProject;
use common::wiremock::{
    AssetSearchBody, AssetSummary, MockApi, StoreAssetSummary, StoreSearchBody,
};

/// The `godot_version` query parameter ggg sent to the mock asset library, if
/// any. `None` means the request omitted the filter entirely.
async fn sent_godot_version(api: &MockApi) -> Option<String> {
    api.received_requests().await.iter().find_map(|r| {
        r.url
            .query_pairs()
            .find(|(k, _)| k == "godot_version")
            .map(|(_, v)| v.into_owned())
    })
}

/// Query parameters ggg sent to the mock store on its search request: the
/// `type` filter and the `compatibility` Godot version, if present.
async fn sent_store_search_params(api: &MockApi) -> Option<(Option<String>, Option<String>)> {
    api.received_requests().await.iter().find_map(|r| {
        if r.url.path() != "/search/query/" {
            return None;
        }
        let pairs: Vec<(String, String)> = r
            .url
            .query_pairs()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let ty = pairs
            .iter()
            .find(|(k, _)| k == "type")
            .map(|(_, v)| v.clone());
        let compatibility = pairs
            .iter()
            .find(|(k, _)| k == "compatibility")
            .map(|(_, v)| v.clone());
        Some((ty, compatibility))
    })
}

/// A store search body with a single named asset.
fn store_body_with(total: u32, slug: &str, name: &str) -> StoreSearchBody {
    StoreSearchBody::new(
        total,
        StoreAssetSummary::new(
            slug,
            "souleat",
            "souleat",
            name,
            "MIT",
            "{base}/asset/souleat/godot-xoshiro256-plus-plus/",
        ),
    )
}

// ---------------------------------------------------------------------------
// AC #1: no results prints 'No results for ...'
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_no_results_prints_message() {
    let api = MockApi::start().await;
    api.mount_asset_search(&AssetSearchBody::new(0, vec![]))
        .await;

    let mut project = TestProject::new();
    project.env_api(&api).config().write();

    project
        .cmd()
        .args(["search", "zzz-nothing", "--source", "asset-library"])
        .assert()
        .success()
        .stdout(contains("No results for \"zzz-nothing\" on Godot 4.3."));
}

// ---------------------------------------------------------------------------
// AC #2: prints table header + result rows
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_prints_table_header_and_result_rows() {
    let api = MockApi::start().await;
    api.mount_asset_search(&AssetSearchBody::new(
        2,
        vec![
            AssetSummary::new(1586, "Terrain Generator", "Lox", "MIT"),
            AssetSummary::new(42, "Toolbox", "Some Author", "CC0"),
        ],
    ))
    .await;

    let mut project = TestProject::new();
    project.env_api(&api).config().write();

    project
        .cmd()
        .args(["search", "terrain", "--source", "asset-library"])
        .assert()
        .success()
        .stdout(
            contains("ID")
                .and(contains("Title"))
                .and(contains("Author"))
                .and(contains("License"))
                .and(contains("1586"))
                .and(contains("Terrain Generator"))
                .and(contains("Lox"))
                .and(contains("MIT"))
                .and(contains("42"))
                .and(contains("Some Author"))
                .and(contains("CC0"))
                .and(contains("ggg add asset-library")),
        );
}

// ---------------------------------------------------------------------------
// AC #3: reads the version filter from ggg.toml
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_uses_ggg_toml_version_as_filter() {
    let api = MockApi::start().await;
    api.mount_asset_search(&AssetSearchBody::new(
        1,
        vec![AssetSummary::new(7, "Anything", "Anyone", "MIT")],
    ))
    .await;

    let mut project = TestProject::new();
    project.env_api(&api).config().godot("4.2-stable").write();

    project
        .cmd()
        .args(["search", "terrain", "--source", "asset-library"])
        .assert()
        .success()
        .stdout(contains("on Godot 4.2"));

    assert_eq!(
        sent_godot_version(&api).await.as_deref(),
        Some("4.2"),
        "the godot_version from ggg.toml must be sent to the asset library"
    );
}

// ---------------------------------------------------------------------------
// AC #4: --godot-version overrides ggg.toml
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_godot_version_flag_overrides_toml() {
    let api = MockApi::start().await;
    api.mount_asset_search(&AssetSearchBody::new(
        1,
        vec![AssetSummary::new(7, "Anything", "Anyone", "MIT")],
    ))
    .await;

    let mut project = TestProject::new();
    project.env_api(&api).config().godot("4.2-stable").write();

    project
        .cmd()
        .args([
            "search",
            "terrain",
            "--source",
            "asset-library",
            "--godot-version",
            "4.3",
        ])
        .assert()
        .success()
        .stdout(contains("on Godot 4.3"));

    assert_eq!(
        sent_godot_version(&api).await.as_deref(),
        Some("4.3"),
        "--godot-version must win over the version in ggg.toml"
    );
}

// ---------------------------------------------------------------------------
// AC #5: works without ggg.toml (no version filter)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_without_ggg_toml_sends_no_version_filter() {
    let api = MockApi::start().await;
    api.mount_asset_search(&AssetSearchBody::new(
        1,
        vec![AssetSummary::new(7, "Anything", "Anyone", "MIT")],
    ))
    .await;

    // Deliberately no ggg.toml: search must still work, without a version
    // filter and without printing a version label.
    let mut project = TestProject::new();
    project.env_api(&api);

    project
        .cmd()
        .args(["search", "terrain", "--source", "asset-library"])
        .assert()
        .success()
        .stdout(contains("on Godot").not());

    assert_eq!(
        sent_godot_version(&api).await,
        None,
        "without ggg.toml the godot_version parameter must be omitted entirely"
    );
}

// ---------------------------------------------------------------------------
// AC #6: 'Showing X of Y' truncation message
// ---------------------------------------------------------------------------

#[tokio::test]
async fn search_showing_x_of_y_truncation_message() {
    let api = MockApi::start().await;
    api.mount_asset_search(&AssetSearchBody::new(
        7,
        vec![
            AssetSummary::new(1, "Alpha", "Author A", "MIT"),
            AssetSummary::new(2, "Beta", "Author B", "MIT"),
            AssetSummary::new(3, "Gamma", "Author C", "MIT"),
        ],
    ))
    .await;

    let mut project = TestProject::new();
    project.env_api(&api).config().write();

    project
        .cmd()
        .args(["search", "terrain", "--source", "asset-library"])
        .assert()
        .success()
        .stdout(contains("Showing 3 of 7 results on Godot 4.3."));
}

// ---------------------------------------------------------------------------
// Asset Store (default source)
// ---------------------------------------------------------------------------

/// The default `--source` is the Asset Store: it renders publisher/slug/
/// Name/License, sends type=0 + compatibility from ggg.toml, and hints at
/// `ggg add asset-store`.
#[tokio::test]
async fn search_default_source_is_asset_store() {
    let api = MockApi::start().await;
    api.mount_store_search(&store_body_with(
        1,
        "godot-xoshiro256-plus-plus",
        "GodotXoshiro256++",
    ))
    .await;

    let mut project = TestProject::new();
    project.env_store_api(&api).config().write();

    project
        .cmd()
        .args(["search", "xoshiro"])
        .assert()
        .success()
        .stdout(
            contains("publisher/slug")
                .and(contains("Name"))
                .and(contains("License"))
                .and(contains("souleat/godot-xoshiro256-plus-plus"))
                .and(contains("GodotXoshiro256++"))
                .and(contains("MIT"))
                .and(contains("on Godot 4.3"))
                .and(contains("ggg add asset-store")),
        );

    let (ty, compatibility) = sent_store_search_params(&api)
        .await
        .expect("store search request");
    assert_eq!(
        ty.as_deref(),
        Some("0"),
        "search must filter to addons via type=0"
    );
    assert_eq!(
        compatibility.as_deref(),
        Some("4.3"),
        "the store must receive the project Godot version as the compatibility param"
    );
}

/// `--source asset-store` is accepted explicitly and behaves like the default.
#[tokio::test]
async fn search_explicit_asset_store_source() {
    let api = MockApi::start().await;
    api.mount_store_search(&store_body_with(
        1,
        "godot-xoshiro256-plus-plus",
        "GodotXoshiro256++",
    ))
    .await;

    let mut project = TestProject::new();
    project.env_store_api(&api).config().write();

    project
        .cmd()
        .args(["search", "xoshiro", "--source", "asset-store"])
        .assert()
        .success()
        .stdout(
            contains("publisher/slug")
                .and(contains("GodotXoshiro256++"))
                .and(contains("ggg add asset-store")),
        );
}

/// Without ggg.toml the store search omits the compatibility param and prints
/// no "on Godot" label.
#[tokio::test]
async fn search_store_without_ggg_toml_sends_no_compatibility() {
    let api = MockApi::start().await;
    api.mount_store_search(&store_body_with(
        1,
        "godot-xoshiro256-plus-plus",
        "GodotXoshiro256++",
    ))
    .await;

    let mut project = TestProject::new();
    project.env_store_api(&api);

    project
        .cmd()
        .args(["search", "xoshiro"])
        .assert()
        .success()
        .stdout(contains("on Godot").not());

    let (ty, compatibility) = sent_store_search_params(&api)
        .await
        .expect("store search request");
    assert_eq!(ty.as_deref(), Some("0"));
    assert_eq!(
        compatibility, None,
        "without ggg.toml the compatibility parameter must be omitted entirely"
    );
}

/// The store source also shows the "Showing X of Y" truncation message.
#[tokio::test]
async fn search_store_showing_x_of_y_truncation_message() {
    let api = MockApi::start().await;
    api.mount_store_search(&store_body_with(
        7,
        "godot-xoshiro256-plus-plus",
        "GodotXoshiro256++",
    ))
    .await;

    let mut project = TestProject::new();
    project.env_store_api(&api).config().write();

    project
        .cmd()
        .args(["search", "xoshiro"])
        .assert()
        .success()
        .stdout(contains("Showing 1 of 7 results on Godot 4.3."));
}

/// Invalid `--source` values are rejected by clap.
#[test]
fn search_rejects_invalid_source() {
    TestProject::new()
        .cmd()
        .args(["search", "terrain", "--source", "asset-lib"])
        .assert()
        .failure()
        .stderr(contains("asset-lib"));
}
