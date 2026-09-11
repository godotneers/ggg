//! Integration tests for `ggg update`.
//!
//! `ggg update` checks whether a newer version of a Godot Asset Library
//! dependency is available by comparing the `asset_version` locked in
//! `ggg.lock` against the current version reported by the Asset Library API.
//! When a newer version exists it drops the lock entry so the next `ggg sync`
//! fetches it; `--dry-run` reports the update without touching `ggg.lock`.

mod common;

use predicates::str::contains;

use common::TestProject;
use common::archive::zip_bytes;
use common::wiremock::{AssetDetailBody, MockApi};

// ---------------------------------------------------------------------------
// Offline paths: no network, no `ggg sync` needed
// ---------------------------------------------------------------------------

/// AC #1: without a `ggg.toml` the command fails with a hint to run `ggg init`.
#[test]
fn update_fails_without_ggg_toml() {
    // With no ggg.toml there is neither a lock file nor any declared
    // dependency to check, so update must refuse to do anything rather than
    // guess -- and point the user at `ggg init`.
    let project = TestProject::new();

    project
        .cmd()
        .arg("update")
        .assert()
        .failure()
        .stderr(contains("no ggg.toml found in the current directory"));
}

/// AC #2: an unknown dependency name errors instead of being silently skipped.
#[test]
fn update_unknown_dep_name_errors() {
    // Typos must not be swallowed: asking for a dependency that is not in
    // ggg.toml is a hard error so the user notices the wrong name.
    let project = TestProject::new();
    project.config().asset("foo", 1586).write();

    project
        .cmd()
        .args(["update", "nonexistent"])
        .assert()
        .failure()
        .stderr(contains("no dependency named \"nonexistent\" in ggg.toml"));
}

/// AC #3: non-asset-library dependencies are rejected with a helpful message.
#[test]
fn update_non_asset_lib_dep_rejected() {
    // Git and archive dependencies cannot be "updated" against the asset
    // library -- their version lives in ggg.toml, not in a remote counter.
    // The command must say so instead of querying the asset library.
    let project = TestProject::new();
    project
        .config()
        .git("foo", "https://example.com/foo", "v1.0.0")
        .write();

    project
        .cmd()
        .args(["update", "foo"])
        .assert()
        .failure()
        .stderr(contains("is not a Godot Asset Library dependency"));
}

/// AC #4: with no asset-library deps in `ggg.toml`, the command says so.
#[test]
fn update_no_asset_lib_deps_reports() {
    // `ggg update` without a name checks every asset-library dependency, so a
    // project that only has git deps has nothing to check. That is not an
    // error -- it reports the empty selection and exits successfully.
    let project = TestProject::new();
    project
        .config()
        .git("foo", "https://example.com/foo", "v1.0.0")
        .write();

    project
        .cmd()
        .arg("update")
        .assert()
        .success()
        .stdout(contains("No asset library dependencies in ggg.toml."));
}

/// AC #5: an asset-lib dep with no lock entry is reported instead of queried.
#[test]
fn update_unlocked_asset_lib_dep_reports_sync_hint() {
    // Without a lock entry there is no base version to compare against, so the
    // command cannot judge whether an update exists. It says so and points the
    // user at `ggg sync`, which installs and locks the current version. This
    // path never touches the network -- no asset library call is made.
    let project = TestProject::new();
    project.config().asset("foo", 1586).write();

    project
        .cmd()
        .arg("update")
        .assert()
        .success()
        .stdout(contains(
            "foo: no lock entry - run `ggg sync` to install and lock the current version.",
        ));
}

// ---------------------------------------------------------------------------
// Wiremock-backed paths: `ggg sync` seeds the lock file first
// ---------------------------------------------------------------------------

/// The zip archive served as asset 1586's `download_url`; `ggg sync` extracts
/// it into the project.
fn asset_zip() -> Vec<u8> {
    zip_bytes(&[("addons/foo/plugin.gd", "# plugin")])
}

/// An `AssetDetailBody` for asset 1586 at the given `version`.
///
/// When `zip` is provided, the mock serves the download itself
/// ([`AssetDetailBody::new_with_file`]); otherwise the body reports a
/// `download_url` for a file that is never fetched.
fn detail(version: u32, version_string: &str, zip: Option<Vec<u8>>) -> AssetDetailBody {
    match zip {
        Some(zip) => AssetDetailBody::new_with_file(
            1586,
            "Foo Addon",
            "author",
            "MIT",
            version,
            version_string,
            "{base}/asset/1586",
            zip,
        ),
        None => AssetDetailBody::new(
            1586,
            "Foo Addon",
            "author",
            "MIT",
            version,
            version_string,
            "{base}/files/foo.zip",
            "{base}/asset/1586",
        ),
    }
}

/// Start the mock, mount asset 1586 at `version`, and sync the project so
/// `ggg.lock` records that version.
async fn sync_locked_at(version: u32, version_string: &str) -> (MockApi, TestProject) {
    // Sync is the realistic way to seed the lock file: it resolves the dep
    // against the mocked asset library, downloads the zip served by the mock,
    // installs it, and writes `asset_version` into ggg.lock.
    let api = MockApi::start().await;
    api.mount_asset_detail(1586, detail(version, version_string, Some(asset_zip())))
        .await;

    let mut project = TestProject::new();
    project.env_api(&api);
    project.config().asset("foo", 1586).write();

    project.cmd().arg("sync").assert().success();
    let lock = project.read("ggg.lock");
    assert!(
        lock.contains(&format!("asset_version = {version}")),
        "sync should lock asset 1586 at version {version}:\n{lock}"
    );
    (api, project)
}

/// AC #6: when the locked version is current, the dep reports 'up to date'.
#[tokio::test]
async fn update_reports_up_to_date() {
    // The asset library still serves the same version that ggg.lock pins, so
    // the command has nothing to do and reports 'up to date' instead of
    // dropping the lock entry.
    let (_api, project) = sync_locked_at(3, "1.0.0").await;

    project
        .cmd()
        .arg("update")
        .assert()
        .success()
        .stdout(contains("foo: up to date (v1.0.0)."));
}

/// AC #7: a newer version drops the lock entry and saves `ggg.lock`.
#[tokio::test]
async fn update_newer_version_drops_lock_entry() {
    // Simulate the addon author releasing a new version: the mock now serves
    // version 4 while ggg.lock still pins version 3. `ggg update` responds by
    // dropping the stale lock entry so the next `ggg sync` reinstalls, and it
    // persists that change by rewriting ggg.lock.
    let (api, project) = sync_locked_at(3, "1.0.0").await;

    // wiremock resolves stubs in mount order, so clear the old detail stub
    // before serving the newer version.
    api.reset().await;
    api.mount_asset_detail(1586, detail(4, "1.1.0", None)).await;

    project
        .cmd()
        .arg("update")
        .assert()
        .success()
        .stdout(contains(
            "foo: version 3 -> v1.1.0 - run `ggg sync` to install.",
        ));

    // After dropping the entry the lock file must no longer reference foo.
    let lock = project.read("ggg.lock");
    assert!(
        !lock.contains("name = \"foo\""),
        "lock entry for foo should have been dropped: {lock}"
    );
}

/// AC #8: `--dry-run` reports the update without modifying `ggg.lock`.
#[tokio::test]
async fn update_dry_run_reports_without_modifying_lock() {
    // `--dry-run` is the "show, don't touch" mode: it detects the same newer
    // version and reports it, but leaves ggg.lock byte-for-byte untouched so
    // a later real run still has a valid base version to compare against.
    let (api, project) = sync_locked_at(3, "1.0.0").await;
    let lock_before = project.read("ggg.lock");

    api.reset().await;
    api.mount_asset_detail(1586, detail(4, "1.1.0", None)).await;

    project
        .cmd()
        .args(["update", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("foo: update available: version 3 -> v1.1.0."));

    assert_eq!(project.read("ggg.lock"), lock_before);
}
