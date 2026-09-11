//! Library-level smoke tests proving wiremock + the `GGG_*_URL` env overrides
//! let ggg's HTTP client round-trip against a hermetic mock server.
//!
//! These are intentionally lower than the CLI: they call the library functions
//! directly (rather than spawning the `ggg` binary) to isolate just the
//! "fetch from a `GGG_*_URL`-pointed server and parse" path. The command-level
//! behaviour is covered by the per-command suites (TASK-1.5 onward).
//!
//! Because the underlying calls are blocking (`reqwest::blocking`) and wiremock
//! needs a tokio runtime, each mock call is dispatched through
//! `tokio::task::spawn_blocking` so the blocking work runs on the blocking
//! thread pool rather than stalling the async runtime that serves wiremock.

mod common;

use common::wiremock::{AssetDetailBody, AssetSearchBody, AssetSummary, MockApi};
use ggg::godot::asset_lib::{self, AssetDetail, AssetSearchResult};
use ggg::godot::manifest;
use ggg::godot::release::GodotVersion;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn fetch_versions_round_trips_against_wiremock() {
    let api = MockApi::start().await;
    api.mount_manifest(
        r#"
- name: "4.6.2"
  flavor: "stable"
  releases:
    - name: "rc1"
    - name: "rc2"

- name: "4.3"
  flavor: "stable"
"#,
    )
    .await;

    unsafe {
        std::env::set_var(
            ggg::envvars::GODOT_MANIFEST_URL_ENV_VAR,
            api.base_url() + "/manifest",
        )
    };

    let releases = tokio::task::spawn_blocking(manifest::fetch_versions)
        .await
        .unwrap()
        .expect("manifest should fetch and parse against wiremock");

    assert_eq!(releases.len(), 4, "4.6.2: stable+rc2+rc1, 4.3: stable");

    let version_4_6: Vec<_> = releases
        .iter()
        .filter(|r| r.version == GodotVersion::new(4, 6, 2))
        .collect();
    let flavors: Vec<&str> = version_4_6.iter().map(|r| r.flavor.as_str()).collect();
    assert_eq!(flavors, vec!["stable", "rc1", "rc2"]);

    let version_4_3: Vec<_> = releases
        .iter()
        .filter(|r| r.version == GodotVersion::new(4, 3, 0))
        .collect();
    assert_eq!(version_4_3.len(), 1);
    assert!(version_4_3[0].is_stable());

    unsafe { std::env::remove_var(ggg::envvars::GODOT_MANIFEST_URL_ENV_VAR) };
}

#[tokio::test]
#[serial]
async fn asset_lib_search_round_trips_against_wiremock() {
    let api = MockApi::start().await;
    api.mount_asset_search(&AssetSearchBody::new(
        1,
        vec![AssetSummary::new(
            1586,
            "Starter Template",
            "godot-engine",
            "MIT",
        )],
    ))
    .await;

    unsafe { std::env::set_var(ggg::envvars::ASSET_LIB_API_URL_ENV_VAR, api.base_url()) };

    let (results, total): (Vec<AssetSearchResult>, u32) =
        tokio::task::spawn_blocking(|| asset_lib::search("template", "4.3"))
            .await
            .unwrap()
            .expect("search should hit wiremock and parse");

    assert_eq!(total, 1);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].asset_id, 1586);
    assert_eq!(results[0].title, "Starter Template");
    assert_eq!(results[0].author, "godot-engine");
    assert_eq!(results[0].license, "MIT");

    unsafe { std::env::remove_var(ggg::envvars::ASSET_LIB_API_URL_ENV_VAR) };
}

#[tokio::test]
#[serial]
async fn asset_lib_get_asset_round_trips_against_wiremock() {
    let api = MockApi::start().await;
    api.mount_asset_detail(
        1586,
        AssetDetailBody::new(
            1586,
            "Starter Template",
            "godot-engine",
            "MIT",
            3,
            "1.0.0",
            "{base}/files/starter-template.zip",
            "{base}/asset/1586",
        ),
    )
    .await;

    unsafe { std::env::set_var(ggg::envvars::ASSET_LIB_API_URL_ENV_VAR, api.base_url()) };

    let asset: AssetDetail = tokio::task::spawn_blocking(|| asset_lib::get_asset(1586))
        .await
        .unwrap()
        .expect("get_asset should hit wiremock and parse");

    assert_eq!(asset.asset_id, 1586);
    assert_eq!(asset.title, "Starter Template");
    assert_eq!(asset.version, 3);
    assert_eq!(asset.version_string, "1.0.0");
    assert_eq!(
        asset.download_hash, None,
        "empty download_hash becomes None"
    );
    // The reported download URL self-references the mock server (port injected).
    assert_eq!(
        asset.download_url,
        format!("{}/files/starter-template.zip", api.base_url())
    );

    unsafe { std::env::remove_var(ggg::envvars::ASSET_LIB_API_URL_ENV_VAR) };
}
