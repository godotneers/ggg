//! End-to-end tests for `ggg search`.
//!
//! These drive the compiled `ggg` binary against a wiremock-backed fake of the
//! Godot Asset Library API (see `common::wiremock`) and assert on the rendered
//! table, the version filter pulled from `ggg.toml` / `--godot-version`, and
//! the pagination/truncation message.

mod common;

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

use common::TestProject;
use common::wiremock::{AssetSearchBody, AssetSummary, MockApi};

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
        .args(["search", "zzz-nothing"])
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
        .args(["search", "terrain"])
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
                .and(contains("CC0")),
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
        .args(["search", "terrain"])
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
        .args(["search", "terrain", "--godot-version", "4.3"])
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
        .args(["search", "terrain"])
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
        .args(["search", "terrain"])
        .assert()
        .success()
        .stdout(contains("Showing 3 of 7 results on Godot 4.3."));
}
