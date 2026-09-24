//! Integration tests for `ggg ls-dep`.
//!
//! `ggg ls-dep` lists the raw contents of a dependency's cache entry: the
//! source tree *before* `strip_components`/`map` are applied. The cache is
//! seeded the realistic way - by running `ggg sync` first - so the `ls-dep`
//! call itself resolves from `ggg.lock` + the cache and never downloads.
//!
//! Git dependencies come from local `file://` bare repositories (see
//! `common::git_fixtures`); archive and asset-library dependencies are served
//! as raw zip bytes by a wiremock server (see `common::wiremock` and
//! `common::archive`).

mod common;

use predicates::str::contains;
use sha2::{Digest, Sha256};

use common::TestProject;
use common::archive::zip_bytes;
use common::git_fixtures::BareRepo;
use common::wiremock::{AssetDetailBody, MockApi, StoreArchive};

// ---------------------------------------------------------------------------
// AC #1: fails with 'no ggg.toml found' when no config exists
// ---------------------------------------------------------------------------

#[test]
fn ls_dep_fails_without_ggg_toml() {
    // Without ggg.toml there is no declared dependency to inspect, so the
    // command must refuse rather than guess.
    let project = TestProject::new();

    project
        .cmd()
        .args(["ls-dep", "my-addon"])
        .assert()
        .failure()
        .stderr(contains("no ggg.toml found"));
}

// ---------------------------------------------------------------------------
// AC #2: unknown dependency name errors
// ---------------------------------------------------------------------------

#[test]
fn ls_dep_fails_for_unknown_dependency() {
    // The name must match a declared dependency; anything else is a hard
    // error before any resolution or cache work happens.
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", "https://example.com/my-addon", "main")
        .write();

    project
        .cmd()
        .args(["ls-dep", "nonexistent"])
        .assert()
        .failure()
        .stderr(contains("dependency \"nonexistent\" not found in ggg.toml"));
}

// ---------------------------------------------------------------------------
// Git dependency: collapsed tree + header, --all flat listing
// ---------------------------------------------------------------------------

/// AC #3 + #5: a cached git dep prints a collapsed tree headed by
/// `<name>  (<rev> -> <sha[:8]>...)`.
#[test]
fn ls_dep_git_shows_collapsed_tree_with_rev_sha_header() {
    let repo = BareRepo::builder()
        .with("addons/my-addon/plugin.gd", "# plugin")
        .with("addons/my-addon/plugin.cfg", "[plugin]")
        .build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    // Seed the cache and lock file the realistic way.
    project.cmd().arg("sync").assert().success();

    let assert = project
        .cmd()
        .args(["ls-dep", "my-addon"])
        .assert()
        .success();
    assert
        // Header: `<name>  (main -> <sha[:8]>...)`.
        .stdout(contains(format!(
            "my-addon  (main -> {}...)",
            &repo.sha()[..8]
        )))
        // Collapsed tree: directories with their direct-file counts.
        .stdout(contains("addons/"))
        .stdout(contains("my-addon/  2 files"));
}

/// AC #4: `--all` lists every file path individually under the same header.
#[test]
fn ls_dep_git_all_lists_every_file_path() {
    let repo = BareRepo::builder()
        .with("addons/my-addon/plugin.gd", "# plugin")
        .with("addons/my-addon/plugin.cfg", "[plugin]")
        .build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    project.cmd().arg("sync").assert().success();

    let assert = project
        .cmd()
        .args(["ls-dep", "my-addon", "--all"])
        .assert()
        .success();
    assert
        .stdout(contains(format!(
            "my-addon  (main -> {}...)",
            &repo.sha()[..8]
        )))
        .stdout(contains("addons/my-addon/plugin.gd"))
        .stdout(contains("addons/my-addon/plugin.cfg"));
}

// ---------------------------------------------------------------------------
// Archive dependency: collapsed tree + header, --all flat listing
// ---------------------------------------------------------------------------

/// The zip archive used for archive ls-dep tests.
fn archive_bytes() -> Vec<u8> {
    zip_bytes(&[
        ("addons/my-archive/plugin.gd", "# plugin"),
        ("addons/my-archive/plugin.cfg", "[plugin]"),
    ])
}

/// AC #3 + #6: a cached archive dep prints a collapsed tree headed by
/// `<name>  (<sha[:8]>...)`, where the sha is the archive's SHA-256.
#[tokio::test]
async fn ls_dep_archive_shows_collapsed_tree_with_sha_header() {
    let zipped = archive_bytes();
    let sha = format!("{:x}", Sha256::digest(&zipped));

    let api = MockApi::start().await;
    api.mount_file("/files/addon.zip", zipped).await;

    let project = TestProject::new();
    project
        .config()
        .archive("my-archive", format!("{}/files/addon.zip", api.base_url()))
        .write();

    project.cmd().arg("sync").assert().success();

    let assert = project
        .cmd()
        .args(["ls-dep", "my-archive"])
        .assert()
        .success();
    assert
        // Header: `<name>  (<sha[:8]>...)`.
        .stdout(contains(format!("my-archive  ({}...)", &sha[..8])))
        .stdout(contains("addons/"))
        .stdout(contains("my-archive/  2 files"));
}

/// AC #4 (archive): `--all` lists every file path individually.
#[tokio::test]
async fn ls_dep_archive_all_lists_every_file_path() {
    let zipped = archive_bytes();
    let sha = format!("{:x}", Sha256::digest(&zipped));

    let api = MockApi::start().await;
    api.mount_file("/files/addon.zip", zipped).await;

    let project = TestProject::new();
    project
        .config()
        .archive("my-archive", format!("{}/files/addon.zip", api.base_url()))
        .write();

    project.cmd().arg("sync").assert().success();

    let assert = project
        .cmd()
        .args(["ls-dep", "my-archive", "--all"])
        .assert()
        .success();
    assert
        .stdout(contains(format!("my-archive  ({}...)", &sha[..8])))
        .stdout(contains("addons/my-archive/plugin.gd"))
        .stdout(contains("addons/my-archive/plugin.cfg"));
}

// ---------------------------------------------------------------------------
// Asset Library dependency: header format
// ---------------------------------------------------------------------------

/// Asset-lib deps list the cache contents too; their header is
/// `asset #<id> v<version> -> <sha[:8]>...`.
#[tokio::test]
async fn ls_dep_asset_header_shows_id_version_and_sha() {
    let zipped = zip_bytes(&[("starter/plugin.gd", "# plugin")]);
    let sha = format!("{:x}", Sha256::digest(&zipped));

    let api = MockApi::start().await;
    // Attaching the archive lets the mock own the download URL and serve the
    // zip so `ggg sync` can download the archive in the same run.
    api.mount_asset_detail(
        1586,
        AssetDetailBody::new_with_file(
            1586,
            "Starter Template",
            "godot-engine",
            "MIT",
            3,
            "1.0.0",
            "{base}/asset/1586",
            zipped,
        ),
    )
    .await;

    let mut project = TestProject::new();
    project.env_api(&api);
    project.config().asset_lib("my-asset", 1586).write();

    project.cmd().arg("sync").assert().success();

    project
        .cmd()
        .args(["ls-dep", "my-asset"])
        .assert()
        .success()
        .stdout(contains(format!(
            "my-asset  (asset #1586 v3 -> {}...)",
            &sha[..8]
        )));
}

// ---------------------------------------------------------------------------
// Asset Store dependency: header format
// ---------------------------------------------------------------------------

/// Store deps list the cache contents too; their header is
/// `publisher/asset v<version> -> <sha[:8]>...`.
#[tokio::test]
async fn ls_dep_store_header_shows_publisher_asset_version() {
    let zipped = zip_bytes(&[("addon/addon-file.txt", "addon content")]);
    let sha = format!("{:x}", Sha256::digest(&zipped));

    let api = MockApi::start().await;
    // Mounting the releases endpoint with the attached archive lets ggg resolve
    // the pinned version and download the zip in the same sync run.
    api.mount_store_releases_with_archives(
        "souleat",
        "photon-torpedo",
        &[StoreArchive::new(2, "1.2.3", true, "4.0", None, zipped)],
    )
    .await;

    let mut project = TestProject::new();
    project.env_store_api(&api);
    project
        .config()
        .asset_store("my-addon", "souleat/photon-torpedo:1.2.3")
        .write();

    project.cmd().arg("sync").assert().success();

    project
        .cmd()
        .args(["ls-dep", "my-addon", "--all"])
        .assert()
        .success()
        .stdout(contains(format!(
            "my-addon  (souleat/photon-torpedo v1.2.3 -> {}...)",
            &sha[..8]
        )));
}
