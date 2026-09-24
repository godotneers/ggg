//! Integration tests for `ggg update`.
//!
//! `ggg update` checks whether a newer version of a Godot Asset Library or
//! Asset Store dependency is available by comparing the version locked in
//! `ggg.lock` against the newest version reported by the relevant API. When a
//! newer version exists it drops the lock entry so the next `ggg sync` fetches
//! it (store deps also get their pinned `ggg.toml` version bumped);
//! `--dry-run` reports the update without touching `ggg.lock` or `ggg.toml`.

mod common;

use predicates::str::contains;

use common::TestProject;
use common::archive::zip_bytes;
use common::wiremock::{AssetDetailBody, MockApi, StoreArchive, StoreRelease};

/// A 40-char git commit sha placeholder.
const GIT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// A 64-char sha-256 archive digest placeholder.
const ARCHIVE_SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const DOWNLOAD_URL: &str = "https://example.com/foo.zip";

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
    project.config().asset_lib("foo", 1586).write();

    project
        .cmd()
        .args(["update", "nonexistent"])
        .assert()
        .failure()
        .stderr(contains("no dependency named \"nonexistent\" in ggg.toml"));
}

/// AC #3: non-asset-library/store dependencies are rejected with a helpful message.
#[test]
fn update_non_asset_lib_dep_rejected() {
    // Git and archive dependencies cannot be "updated" against the asset
    // library or store -- their version lives in ggg.toml, not in a remote
    // counter. The command must say so instead of querying a remote.
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
        .stderr(contains(
            "is not a Godot Asset Library or Asset Store dependency",
        ));
}

/// AC #4: with no asset-lib/store deps in `ggg.toml`, the command says so.
#[test]
fn update_no_asset_lib_deps_reports() {
    // `ggg update` without a name checks every eligible dependency, so a
    // project that only has git deps has nothing to check. That is not an
    // error -- it reports the empty selection and exits successfully. (The
    // git dep is locked so the preflight `ggg sync` requirement is satisfied.)
    let project = TestProject::new();
    project
        .config()
        .git("foo", "https://example.com/foo", "v1.0.0")
        .write();
    project
        .lock()
        .git("foo", "https://example.com/foo", "v1.0.0", GIT_SHA)
        .write();

    project
        .cmd()
        .arg("update")
        .assert()
        .success()
        .stdout(contains(
            "No asset library or asset store dependencies in ggg.toml.",
        ));
}

/// update enforces a sync-before-update rule for every dependency kind: a
/// git dep that has never been synced aborts the whole-project update.
#[test]
fn update_unlocked_git_dep_fails_with_sync_hint() {
    let project = TestProject::new();
    project
        .config()
        .git("foo", "https://example.com/foo", "v1.0.0")
        .write();

    project
        .cmd()
        .arg("update")
        .assert()
        .failure()
        .stderr(contains("out of sync"))
        .stderr(contains(
            "foo: no lock entry - run `ggg sync` to install and lock the current version.",
        ));
}

/// AC #5: an asset-lib dep with no lock entry aborts with a sync hint.
#[test]
fn update_unlocked_asset_lib_dep_fails_with_sync_hint() {
    // Without a lock entry there is no base version to compare against, so the
    // command cannot judge whether an update exists. It refuses to run, points
    // the user at `ggg sync`, and - because this happens during the drift
    // preflight - never touches the network. Nothing is written.
    let project = TestProject::new();
    project.config().asset_lib("foo", 1586).write();

    project
        .cmd()
        .arg("update")
        .assert()
        .failure()
        .stderr(contains(
            "foo: no lock entry - run `ggg sync` to install and lock the current version.",
        ));
}

/// AC #5: a store dep with no lock entry aborts with a sync hint.
///
/// The pinned version has no base to compare against until `ggg sync` runs, so
/// the command refuses to run and reports it -- and because it never looks the
/// lock entry up in the store, no network call is made.
#[test]
fn update_unlocked_store_dep_fails_with_sync_hint() {
    let project = TestProject::new();
    project
        .config()
        .asset_store("my-addon", "souleat/my-addon:1.0.0")
        .write();

    project
        .cmd()
        .arg("update")
        .assert()
        .failure()
        .stderr(contains(
            "my-addon: no lock entry - run `ggg sync` to install and lock the current version.",
        ));
}

/// A stale lock entry (removed from `ggg.toml` but still in `ggg.lock`)
/// aborts the update instead of being silently ignored.
#[test]
fn update_stale_lock_entry_fails() {
    // An orphaned lock entry means ggg.lock and ggg.toml disagree about what
    // the project pins. Updating against that file could miss dependencies
    // that should be checked, so the command aborts and points at `ggg sync`
    // (which prunes the stale entry). No writes happen.
    let project = TestProject::new();
    project.config().asset_lib("foo", 1586).write();
    project
        .lock()
        .git("orphan", "https://example.com/orphan.git", "main", GIT_SHA)
        .asset_lib("foo", 1586, 1, DOWNLOAD_URL, ARCHIVE_SHA)
        .write();

    project
        .cmd()
        .arg("update")
        .assert()
        .failure()
        .stderr(contains("orphan: lock entry (git) is not in ggg.toml"));
}

/// A dependency whose source kind changed (here: store -> git in the lock
/// file) aborts the update instead of comparing against the wrong version.
#[test]
fn update_kind_mismatch_fails() {
    // The lock file pins `foo` as a store release but ggg.toml now declares it
    // as a git dependency. The locked version cannot be trusted, so the update
    // aborts; `ggg sync` rewrites the entry for the new kind.
    let project = TestProject::new();
    project
        .config()
        .asset_store("foo", "souleat/foo:1.0.0")
        .write();
    project
        .lock()
        .git("foo", "https://example.com/foo.git", "main", GIT_SHA)
        .write();

    project
        .cmd()
        .arg("update")
        .assert()
        .failure()
        .stderr(contains(
            "foo: locked as a git dependency but ggg.toml declares it as a asset-store dependency",
        ));
}

/// A lock-key field edited in `ggg.toml` (here: the store release pin) aborts
/// the update instead of comparing against a self-consistently wrong pin.
#[test]
fn update_store_pin_edit_fails() {
    // The user edited the pinned version in ggg.toml without syncing: the lock
    // file still records the previously installed 1.0.0. The update command
    // cannot trust either number, so it aborts and asks for `ggg sync` first.
    let project = TestProject::new();
    project
        .config()
        .asset_store("my-addon", "souleat/my-addon:2.0.0")
        .write();
    project
        .lock()
        .asset_store(
            "my-addon",
            "souleat",
            "my-addon",
            3,
            "1.0.0",
            DOWNLOAD_URL,
            ARCHIVE_SHA,
        )
        .write();

    project
        .cmd()
        .arg("update")
        .assert()
        .failure()
        .stderr(contains(
            "my-addon: locked release 1.0.0 does not match the version 2.0.0 pinned in ggg.toml",
        ));
}

/// The named form validates drift for just the requested dependency: an
/// out-of-sync lock file fails it before any store/asset-library API call.
#[test]
fn update_named_dep_drift_fails_without_network() {
    // `update <name>` checks only the named dependency against its lock entry.
    // With a missing lock entry the base version is unknown, so the command
    // refuses to run. Stale-lock or kind-swap drift would fail the same way;
    // neither path performs an API call, so this runs offline.
    let project = TestProject::new();
    project.config().asset_lib("foo", 1586).write();

    project
        .cmd()
        .args(["update", "foo"])
        .assert()
        .failure()
        .stderr(contains(
            "foo: no lock entry - run `ggg sync` to install and lock the current version.",
        ));
}

/// A partially synced project (one checkable dep + one unlocked dep) fails
/// atomically: nothing is written, even for the dep that would have an update.
#[tokio::test]
async fn update_fails_atomically_when_any_dep_is_unlocked() {
    // Dep `a` is synced at 1.0.0 and would report an update; dep `b` has no
    // lock entry at all. The drift preflight aborts before any API call, so
    // `a` is never even queried and no file changes on disk.
    let api = MockApi::start().await;
    api.mount_store_releases(
        "souleat",
        "aaa",
        &[StoreRelease::new(
            10,
            "1.1.0",
            true,
            "https://example.com/aaa.zip",
            "4.3",
            None,
        )],
    )
    .await;

    let mut project = TestProject::new();
    project.env_api(&api);
    project
        .config()
        .asset_store("aaa", "souleat/aaa:1.0.0")
        .asset_store("bbb", "souleat/bbb:1.0.0")
        .write();
    project
        .lock()
        .asset_store(
            "aaa",
            "souleat",
            "aaa",
            1,
            "1.0.0",
            DOWNLOAD_URL,
            ARCHIVE_SHA,
        )
        .write();

    let before_toml = project.read("ggg.toml");
    let before_lock = project.read("ggg.lock");

    project
        .cmd()
        .arg("update")
        .assert()
        .failure()
        .stderr(contains(
            "bbb: no lock entry - run `ggg sync` to install and lock the current version.",
        ));

    assert_eq!(
        project.read("ggg.toml"),
        before_toml,
        "ggg.toml must be untouched"
    );
    assert_eq!(
        project.read("ggg.lock"),
        before_lock,
        "ggg.lock must be untouched"
    );
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
    project.config().asset_lib("foo", 1586).write();

    project.cmd().arg("sync").assert().success();
    let lock = project.read_ggg_lock();
    let entry = lock
        .entries
        .iter()
        .find(|e| e.name == "foo")
        .expect("sync should record an entry for foo");
    assert_eq!(
        entry.asset_version,
        Some(version),
        "sync should lock asset 1586 at version {version}"
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
    assert!(
        !project
            .read_ggg_lock()
            .entries
            .iter()
            .any(|e| e.name == "foo"),
        "lock entry for foo should have been dropped"
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

// ---------------------------------------------------------------------------
// Wiremock-backed Asset Store paths: `ggg sync` seeds the lock file first
// ---------------------------------------------------------------------------

/// A stable, Godot-4.x-compatible store release upload for `souleat/my-addon`.
fn store_archive(id: u64, version: &str) -> StoreArchive {
    StoreArchive::new(
        id,
        version,
        true,
        "4.0",
        None,
        zip_bytes(&[("addon/plugin.gd", "# plugin")]),
    )
}

/// Mount the `souleat/my-addon` releases endpoint and sync the project so
/// `ggg.lock` records the pinned version.
async fn sync_store_locked_at(spec: &str, archives: &[StoreArchive]) -> (MockApi, TestProject) {
    let api = MockApi::start().await;
    api.mount_store_releases_with_archives("souleat", "my-addon", archives)
        .await;

    let mut project = TestProject::new();
    project.env_store_api(&api);
    project.config().asset_store("my-addon", spec).write();

    project.cmd().arg("sync").assert().success();
    let lock = project.read_ggg_lock();
    let entry = lock
        .entries
        .iter()
        .find(|e| e.name == "my-addon")
        .expect("sync should record an entry for my-addon");
    assert_eq!(
        entry.publisher_slug.as_deref(),
        Some("souleat"),
        "sync should lock the store dep"
    );
    (api, project)
}

/// AC #6: a store dep whose pinned version is already the newest stable
/// Godot-compatible release reports 'up to date'.
#[tokio::test]
async fn update_store_reports_up_to_date() {
    let (_api, project) =
        sync_store_locked_at("souleat/my-addon:1.0.0", &[store_archive(1, "1.0.0")]).await;

    project
        .cmd()
        .arg("update")
        .assert()
        .success()
        .stdout(contains("my-addon: up to date (v1.0.0)."));
}

/// AC #7: a newer stable release drops the lock entry and bumps the pinned
/// version in `ggg.toml`.
#[tokio::test]
async fn update_store_newer_version_drops_lock_and_bumps_pin() {
    // Simulate the maintainer releasing 2.0.0: the mock now serves it while
    // ggg.lock and ggg.toml still pin 1.0.0. `ggg update` responds by bumping
    // the pinned version in ggg.toml and dropping the stale lock entry so the
    // next `ggg sync` downloads the new release.
    let (api, project) =
        sync_store_locked_at("souleat/my-addon:1.0.0", &[store_archive(1, "1.0.0")]).await;

    api.reset().await;
    api.mount_store_releases(
        "souleat",
        "my-addon",
        &[StoreRelease::new(
            2,
            "2.0.0",
            true,
            "https://example.com/files/store-2.zip",
            "4.0",
            None,
        )],
    )
    .await;

    project
        .cmd()
        .arg("update")
        .assert()
        .success()
        .stdout(contains(
            "my-addon: version 1.0.0 -> v2.0.0 - run `ggg sync` to install.",
        ));

    assert!(
        !project
            .read_ggg_lock()
            .entries
            .iter()
            .any(|e| e.name == "my-addon"),
        "lock entry for my-addon should have been dropped"
    );

    let toml = project.read("ggg.toml");
    assert!(
        toml.contains("asset_store_asset = \"souleat/my-addon:2.0.0\""),
        "ggg.toml should pin the newer version: {toml}"
    );
    assert!(
        !toml.contains(":1.0.0"),
        "ggg.toml should no longer pin 1.0.0: {toml}"
    );
}

/// AC #8: `--dry-run` reports a store update (with release id + version)
/// without modifying `ggg.lock` or `ggg.toml`.
#[tokio::test]
async fn update_store_dry_run_reports_without_mutating_files() {
    let (api, project) =
        sync_store_locked_at("souleat/my-addon:1.0.0", &[store_archive(1, "1.0.0")]).await;
    let lock_before = project.read("ggg.lock");
    let toml_before = project.read("ggg.toml");

    api.reset().await;
    api.mount_store_releases(
        "souleat",
        "my-addon",
        &[StoreRelease::new(
            2,
            "2.0.0",
            true,
            "https://example.com/files/store-2.zip",
            "4.0",
            None,
        )],
    )
    .await;

    project
        .cmd()
        .args(["update", "--dry-run"])
        .assert()
        .success()
        .stdout(contains(
            "my-addon: update available: version 1.0.0 -> v2.0.0 (release id 2).",
        ));

    assert_eq!(project.read("ggg.lock"), lock_before);
    assert_eq!(project.read("ggg.toml"), toml_before);
}

/// AC #9: "newer" is decided by version, not release id - a later-uploaded
/// patch of an old series never downgrades a newer series.
///
/// The maintainer released 5.0.0 (id 120) and then back-patched the 3.x series
/// as 3.9.2 (id 121). A project pinned at 4.0.4 must update to 5.0.0, not
/// "downgrade" to the higher-id 3.9.2.
#[tokio::test]
async fn update_store_prefers_higher_version_over_higher_release_id() {
    let (api, project) = sync_store_locked_at(
        "souleat/my-addon:4.0.4",
        &[
            store_archive(119, "4.0.4"),
            store_archive(120, "5.0.0"),
            store_archive(121, "3.9.2"),
        ],
    )
    .await;

    api.reset().await;
    api.mount_store_releases(
        "souleat",
        "my-addon",
        &[
            StoreRelease::new(
                119,
                "4.0.4",
                true,
                "https://example.com/4.0.4.zip",
                "4.0",
                None,
            ),
            StoreRelease::new(
                120,
                "5.0.0",
                true,
                "https://example.com/5.0.0.zip",
                "4.0",
                None,
            ),
            StoreRelease::new(
                121,
                "3.9.2",
                true,
                "https://example.com/3.9.2.zip",
                "4.0",
                None,
            ),
        ],
    )
    .await;

    project
        .cmd()
        .arg("update")
        .assert()
        .success()
        .stdout(contains(
            "my-addon: version 4.0.4 -> v5.0.0 - run `ggg sync` to install.",
        ));

    let toml = project.read("ggg.toml");
    assert!(
        toml.contains("asset_store_asset = \"souleat/my-addon:5.0.0\""),
        "ggg.toml should bump to 5.0.0, never the higher-id 3.9.2: {toml}"
    );
    assert!(
        !toml.contains("3.9.2"),
        "ggg.toml must not pin the higher-id back-patched release: {toml}"
    );
}
