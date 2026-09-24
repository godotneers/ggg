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

use common::wiremock::{
    AssetDetailBody, AssetSearchBody, AssetSummary, MockApi, StoreAssetSummary, StoreRelease,
    StoreSearchBody,
};
use ggg::godot::asset_lib::{self, AssetDetail, AssetSearchResult};
use ggg::godot::asset_store;
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

    let (results, total): (Vec<AssetSearchResult>, u32) = tokio::task::spawn_blocking(|| {
        asset_lib::search("template", Some(&GodotVersion::new(4, 3, 0)))
    })
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

#[tokio::test]
#[serial]
async fn store_search_round_trips_against_wiremock() {
    let api = MockApi::start().await;
    api.mount_store_search(
        &StoreSearchBody::new(
            3,
            StoreAssetSummary::new(
                "godot-xoshiro256-plus-plus",
                "souleat",
                "souleat",
                "GodotXoshiro256++",
                "MIT",
                "{base}/asset/souleat/godot-xoshiro256-plus-plus/",
            ),
        )
        .with_scroll(Some("scroll-token-1".to_string())),
    )
    .await;

    unsafe { std::env::set_var(ggg::envvars::ASSET_STORE_API_URL_ENV_VAR, api.base_url()) };

    let (results, total): (Vec<asset_store::StoreAsset>, u32) = tokio::task::spawn_blocking(|| {
        asset_store::search("template", Some(&GodotVersion::new(4, 3, 0)))
    })
    .await
    .unwrap()
    .expect("store search should hit wiremock and parse");

    // The server reported 3 total matches but a scroll token was present; the
    // client returns only the first page while surfacing the total ("Showing
    // X of Y" is left to the caller).
    assert_eq!(total, 3);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].slug, "godot-xoshiro256-plus-plus");
    assert_eq!(results[0].name, "GodotXoshiro256++");
    assert_eq!(results[0].license, "MIT");
    assert_eq!(results[0].publisher.name, "souleat");
    assert_eq!(results[0].publisher.slug, "souleat");
    assert_eq!(
        results[0].store_url,
        format!(
            "{}/asset/souleat/godot-xoshiro256-plus-plus/",
            api.base_url()
        )
    );

    // type=0 (addons) and the compatibility filter must be sent. The
    // compatibility format is MAJOR.MINOR ("4.3"), confirmed against the live
    // API.
    let requests = api.received_requests().await;
    let search = requests
        .iter()
        .find(|r| r.url.path() == "/search/query/")
        .expect("a search request should have reached the store");
    let params: Vec<(String, String)> = search
        .url
        .query_pairs()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
    assert!(
        params.iter().any(|(k, v)| k == "type" && v == "0"),
        "search must filter to addons via type=0, got {params:?}"
    );
    assert!(
        params
            .iter()
            .any(|(k, v)| k == "compatibility" && v == "4.3"),
        "search must send the major.minor compatibility param, got {params:?}"
    );

    unsafe { std::env::remove_var(ggg::envvars::ASSET_STORE_API_URL_ENV_VAR) };
}

#[tokio::test]
#[serial]
async fn store_get_asset_round_trips_against_wiremock() {
    let api = MockApi::start().await;
    api.mount_store_asset(
        "souleat",
        "godot-xoshiro256-plus-plus",
        &StoreAssetSummary::new(
            "godot-xoshiro256-plus-plus",
            "souleat",
            "souleat",
            "GodotXoshiro256++",
            "MIT",
            "{base}/asset/souleat/godot-xoshiro256-plus-plus/",
        ),
    )
    .await;

    unsafe { std::env::set_var(ggg::envvars::ASSET_STORE_API_URL_ENV_VAR, api.base_url()) };

    let asset: asset_store::StoreAsset = tokio::task::spawn_blocking(|| {
        asset_store::get_asset("souleat", "godot-xoshiro256-plus-plus")
    })
    .await
    .unwrap()
    .expect("get_asset should hit wiremock and parse");

    assert_eq!(asset.slug, "godot-xoshiro256-plus-plus");
    assert_eq!(asset.name, "GodotXoshiro256++");
    assert_eq!(asset.license, "MIT");
    assert_eq!(asset.publisher.slug, "souleat");
    assert_eq!(asset.publisher.name, "souleat");
    assert_eq!(
        asset.store_url,
        format!(
            "{}/asset/souleat/godot-xoshiro256-plus-plus/",
            api.base_url()
        )
    );

    unsafe { std::env::remove_var(ggg::envvars::ASSET_STORE_API_URL_ENV_VAR) };
}

#[tokio::test]
#[serial]
async fn store_get_releases_round_trips_against_wiremock() {
    let api = MockApi::start().await;
    api.mount_store_releases(
        "souleat",
        "godot-xoshiro256-plus-plus",
        &[
            StoreRelease::new(
                5468,
                "0.1.0",
                true,
                "{base}/files/godot-xoshiro256pp-0.1.0.zip",
                "4.0",
                None,
            ),
            StoreRelease::new(
                5469,
                "0.2.0-rc1",
                false,
                "{base}/files/godot-xoshiro256pp-0.2.0-rc1.zip",
                "4.1",
                Some("4.5".to_string()),
            ),
        ],
    )
    .await;

    unsafe { std::env::set_var(ggg::envvars::ASSET_STORE_API_URL_ENV_VAR, api.base_url()) };

    let releases: Vec<asset_store::StoreRelease> = tokio::task::spawn_blocking(|| {
        asset_store::get_releases(
            "souleat",
            "godot-xoshiro256-plus-plus",
            Some(&GodotVersion::new(4, 3, 0)),
            true,
        )
    })
    .await
    .unwrap()
    .expect("get_releases should hit wiremock and parse");

    assert_eq!(releases.len(), 2);
    assert_eq!(releases[0].id, 5468);
    assert_eq!(releases[0].version, "0.1.0");
    assert!(releases[0].stable);
    assert_eq!(releases[0].min_godot_version, "4.0");
    assert_eq!(releases[0].max_godot_version, None);
    assert_eq!(
        releases[0].download_url,
        format!("{}/files/godot-xoshiro256pp-0.1.0.zip", api.base_url())
    );
    assert_eq!(releases[1].version, "0.2.0-rc1");
    assert!(!releases[1].stable);
    assert_eq!(releases[1].max_godot_version.as_deref(), Some("4.5"));

    // The stable_only and compatibility filters are passed through to the
    // server verbatim; the client does not filter locally.
    let requests = api.received_requests().await;
    let releases_req = requests
        .iter()
        .find(|r| r.url.path() == "/releases/souleat/godot-xoshiro256-plus-plus/")
        .expect("a releases request should have reached the store");
    let params: Vec<(String, String)> = releases_req
        .url
        .query_pairs()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
    assert!(
        params
            .iter()
            .any(|(k, v)| k == "compatibility" && v == "4.3"),
        "get_releases must send the major.minor compatibility param, got {params:?}"
    );
    assert!(
        params
            .iter()
            .any(|(k, v)| k == "stable_only" && v == "true"),
        "get_releases must forward stable_only=true, got {params:?}"
    );

    unsafe { std::env::remove_var(ggg::envvars::ASSET_STORE_API_URL_ENV_VAR) };
}
