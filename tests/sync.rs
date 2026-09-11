//! End-to-end tests for `ggg sync`.
//!
//! These drive the compiled `ggg` binary against an isolated project + cache
//! and assert on the files it writes (`ggg.lock`, `.ggg.state`, `.gitignore`,
//! installed dependency files) and on its stdout/stderr/exit status.
//!
//! Git dependencies are served from local `file://` bare repositories (see
//! `common::git_fixtures`), so they need no network. Archive dependencies are
//! served as raw zip bytes by a wiremock server (see `common::wiremock` and
//! `common::archive`).
//!
//! Asset Library dependencies go through the wiremock fake of the Godot Asset
//! Library API (`mount_asset_detail` with a `new_with_file` body serves the
//! download too) and must obey the same ownership/conflict/stale-removal rules
//! as git and archive deps.

mod common;

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

use common::TestProject;
use common::archive::zip_bytes;
use common::archive::{fake_godot_asset, fake_template_tpz};
use common::git_fixtures::BareRepo;
use common::wiremock::{AssetDetailBody, MockApi};
use ggg::godot::release::GodotRelease;

// ---------------------------------------------------------------------------
// AC #1: fails with 'no ggg.toml found' when no config exists
// ---------------------------------------------------------------------------

#[test]
fn sync_fails_without_ggg_toml() {
    // A project without ggg.toml has no declared Godot version or
    // dependencies, so sync must refuse to do anything rather than guess.
    let project = TestProject::new();

    project
        .cmd()
        .arg("sync")
        .assert()
        .failure()
        .stderr(contains("no ggg.toml found"));
}

// ---------------------------------------------------------------------------
// AC #2: empty dependency list + dry-run reports nothing to install
// ---------------------------------------------------------------------------

#[test]
fn sync_dry_run_with_no_deps_reports_nothing() {
    // With no dependencies declared, there is nothing for sync to plan.
    // The dry-run report should therefore stay silent (and succeed) --
    // an empty project must not be treated as an error.
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args(["sync", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("install").not());
}

// ---------------------------------------------------------------------------
// AC #3: install writes ggg.lock, .ggg.state and updates .gitignore
// ---------------------------------------------------------------------------

#[test]
fn sync_installs_and_writes_lock_state_gitignore() {
    // The core promise of `ggg sync`: a git dependency is resolved, its files
    // are installed into the project, and the run is recorded so future runs
    // can detect conflicts and clean up stale files.
    //
    // - the installed file must match the repo content exactly
    // - ggg.lock pins the resolved version
    // - .ggg.state records what ggg installed (ownership tracking)
    // - .ggg.state is added to .gitignore so it is never committed
    let repo = BareRepo::builder().with("plugin.gd", "# hello").build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    project
        .cmd()
        .arg("sync")
        .assert()
        .success()
        .stdout(contains("my-addon"));

    assert_eq!(project.read("plugin.gd"), "# hello");
    assert!(project.exists("ggg.lock"));
    assert!(project.exists(".ggg.state"));

    let gitignore = project.read(".gitignore");
    assert!(gitignore.contains(".ggg.state"));
}

// ---------------------------------------------------------------------------
// AC #4: --dry-run prints the plan without writing files
// ---------------------------------------------------------------------------

#[test]
fn sync_dry_run_prints_plan_but_writes_nothing() {
    // `--dry-run` is the "show me what would change" mode: it must report the
    // install plan but have zero side effects. Nothing is written -- not the
    // dependency files, not ggg.lock/.ggg.state, not even .gitignore.
    let repo = BareRepo::builder().with("plugin.gd", "# hello").build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    project
        .cmd()
        .args(["sync", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("would install"));

    assert!(!project.exists("plugin.gd"));
    assert!(!project.exists("ggg.lock"));
    assert!(!project.exists(".ggg.state"));
    assert!(!project.exists(".gitignore"));
}

// ---------------------------------------------------------------------------
// AC #5: modified owned file blocks without --force
// ---------------------------------------------------------------------------

#[test]
fn sync_modified_owned_file_blocks_without_force() {
    // Ownership rule: a file ggg installed is "owned" only while it still
    // matches what was recorded in .ggg.state. If the user edits such a file
    // (hash differs from state), re-syncing would silently destroy their work,
    // so sync must refuse -- and leave their edit untouched.
    let repo = BareRepo::builder().with("plugin.gd", "# hello").build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    // First sync installs the file and records ownership.
    project.cmd().arg("sync").assert().success();
    // The user edits the installed file.
    project.write("plugin.gd", "# user modification");

    project
        .cmd()
        .arg("sync")
        .assert()
        .failure()
        .stderr(contains("modified since last install"));

    // The user's modification is preserved when sync is blocked.
    assert_eq!(project.read("plugin.gd"), "# user modification");
}

// ---------------------------------------------------------------------------
// AC #6: --force overwrites user-modified owned files
// ---------------------------------------------------------------------------

#[test]
fn sync_force_overwrites_modified_owned_file() {
    // `--force` is the explicit user override for the ownership rule: a
    // modified owned file is overwritten with the dependency's content, so the
    // user can opt into discarding their edit.
    let repo = BareRepo::builder().with("plugin.gd", "# hello").build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    project.cmd().arg("sync").assert().success();
    project.write("plugin.gd", "# user modification");

    project.cmd().args(["sync", "--force"]).assert().success();

    // The user edit is gone; the dependency content is restored.
    assert_eq!(project.read("plugin.gd"), "# hello");
}

// ---------------------------------------------------------------------------
// AC #7: --force overwrites files not under ggg's control
// ---------------------------------------------------------------------------

#[test]
fn sync_force_overwrites_unmanaged_file() {
    // A file that exists at an install destination but is NOT tracked in
    // .ggg.state is "not under ggg's control" -- e.g. a file the user created
    // before ever running sync. Overwriting it would clobber user data, so
    // sync must block. `--force` is required to overwrite it.
    //
    // This is distinct from the "modified owned file" case (AC #5/6): that
    // file is ggg's, just edited; this one was never ggg's.
    let repo = BareRepo::builder().with("plugin.gd", "# hello").build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    // A file sits at an install destination before any state exists, so it is
    // not under ggg's control and blocks sync.
    project.write("plugin.gd", "# user file");

    project
        .cmd()
        .arg("sync")
        .assert()
        .failure()
        .stderr(contains("not under ggg's control"));

    project.cmd().args(["sync", "--force"]).assert().success();

    assert_eq!(project.read("plugin.gd"), "# hello");
}

// ---------------------------------------------------------------------------
// AC #7b: [sync] force_overwrite globs bypass conflict detection
// ---------------------------------------------------------------------------

#[test]
fn sync_force_overwrite_glob_overrides_without_force_flag() {
    // Config-level escape hatch: a `[sync] force_overwrite` glob treats
    // matching paths as always-writable, bypassing conflict detection -- no
    // `--force` flag needed at runtime. This is for files the Godot engine
    // rewrites automatically (e.g. .import metadata, .uid files): engine
    // churn should never count as a conflicting user edit.
    let repo = BareRepo::builder().with("plugin.import", "# hello").build();
    let project = TestProject::new();
    project
        .config()
        .sync_force_overwrite(&["**/*.import"])
        .git("my-addon", repo.file_url(), "main")
        .write();

    project.cmd().arg("sync").assert().success();
    assert_eq!(project.read("plugin.import"), "# hello");

    // Simulate the engine rewriting the installed metadata file; re-sync
    // without --force succeeds because the path matches the glob.
    project.write("plugin.import", "# engine rewrite");

    project.cmd().arg("sync").assert().success();

    assert_eq!(project.read("plugin.import"), "# hello");
}

// ---------------------------------------------------------------------------
// AC #8: stale files cleaned up after removing a dep
// ---------------------------------------------------------------------------

#[test]
fn sync_removes_stale_files_after_dep_removed() {
    // `ggg remove` only edits ggg.toml; cleanup happens on the next sync. A
    // dependency that is no longer declared must have its installed files
    // removed and vanish from .ggg.state, so the project returns to a clean
    // state without leaving orphaned files.
    let repo = BareRepo::builder().with("plugin.gd", "# hello").build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    project.cmd().arg("sync").assert().success();
    assert!(project.exists("plugin.gd"));

    // Remove the dep from ggg.toml (ggg remove does not touch the files yet).
    project
        .cmd()
        .args(["remove", "my-addon"])
        .assert()
        .success();

    // The next sync uninstalls the orphaned file...
    project.cmd().arg("sync").assert().success();

    assert!(!project.exists("plugin.gd"));
    // ...and no longer tracks the removed dep in the state file.
    let state = project.read(".ggg.state");
    assert!(
        !state.contains("my-addon"),
        "state should no longer list removed dep"
    );
}

// ---------------------------------------------------------------------------
// AC #8 (remap): stale files cleaned after a dep is remapped
// ---------------------------------------------------------------------------

#[test]
fn sync_removes_stale_files_after_dep_remapped() {
    // If a dep is repointed to a different source (same name, new URL/rev),
    // the files from the OLD source are no longer part of the install plan.
    // Sync must remove them, not accumulate them alongside the new files.
    let repo_a = BareRepo::builder().with("a.txt", "# from a").build();
    let repo_b = BareRepo::builder().with("b.txt", "# from b").build();

    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo_a.file_url(), "main")
        .write();

    project.cmd().arg("sync").assert().success();
    assert!(project.exists("a.txt"));

    // Remap the dependency to a different source.
    project
        .config()
        .git("my-addon", repo_b.file_url(), "main")
        .write();

    project.cmd().arg("sync").assert().success();

    // The old source's file is cleaned up, the new one is installed.
    assert!(
        !project.exists("a.txt"),
        "stale file should be removed after remap"
    );
    assert_eq!(project.read("b.txt"), "# from b");
}

// ---------------------------------------------------------------------------
// Archive dependency coverage (full behavior parity)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sync_installs_archive_dependency() {
    // Archive deps are HTTP-downloaded (a .zip served by wiremock here),
    // extracted, and installed exactly like git deps. This proves the
    // archive branch of the pipeline: download -> cache -> install, with the
    // download URL pinned in ggg.lock.
    let api = MockApi::start().await;
    api.mount_file(
        "/files/addon.zip",
        zip_bytes(&[("archive-file.txt", "archive content")]),
    )
    .await;

    let project = TestProject::new();
    project
        .config()
        .archive("my-archive", format!("{}/files/addon.zip", api.base_url()))
        .write();

    project
        .cmd()
        .arg("sync")
        .assert()
        .success()
        .stdout(contains("my-archive"));

    // The zip's file is installed at the project root...
    assert_eq!(project.read("archive-file.txt"), "archive content");
    assert!(project.exists("ggg.lock"));

    // ...and the lock file records the archive source URL.
    let lock = project.read("ggg.lock");
    assert!(lock.contains("name = \"my-archive\""));
    assert!(lock.contains("url = \""));
}

#[tokio::test]
async fn sync_archive_modified_owned_blocks_then_force_overwrites() {
    // The ownership/conflict rules must behave identically for archive deps:
    // editing an installed archive file blocks re-sync, and `--force` restores
    // the dependency's content.
    let api = MockApi::start().await;
    api.mount_file(
        "/files/addon.zip",
        zip_bytes(&[("archive-file.txt", "archive content")]),
    )
    .await;

    let project = TestProject::new();
    project
        .config()
        .archive("my-archive", format!("{}/files/addon.zip", api.base_url()))
        .write();

    project.cmd().arg("sync").assert().success();
    project.write("archive-file.txt", "# user change");

    // Modified owned file: blocked without --force.
    project
        .cmd()
        .arg("sync")
        .assert()
        .failure()
        .stderr(contains("modified since last install"));

    // --force discards the edit and restores the installed content.
    project.cmd().args(["sync", "--force"]).assert().success();

    assert_eq!(project.read("archive-file.txt"), "archive content");
}

#[tokio::test]
async fn sync_archive_remap_removes_stale_files() {
    // Remapping an archive dep to a new URL must clean up files from the old
    // URL (same parity as the git remap test): no orphaned files accumulate.
    let api = MockApi::start().await;
    api.mount_file("/files/a.zip", zip_bytes(&[("a.txt", "# from a")]))
        .await;
    api.mount_file("/files/b.zip", zip_bytes(&[("b.txt", "# from b")]))
        .await;

    let project = TestProject::new();
    project
        .config()
        .archive("my-archive", format!("{}/files/a.zip", api.base_url()))
        .write();

    project.cmd().arg("sync").assert().success();
    assert!(project.exists("a.txt"));

    // Repoint the same dep at a different archive.
    project
        .config()
        .archive("my-archive", format!("{}/files/b.zip", api.base_url()))
        .write();

    project.cmd().arg("sync").assert().success();

    // Old URL's file is removed; new URL's file is installed.
    assert!(
        !project.exists("a.txt"),
        "stale file should be removed after URL remap"
    );
    assert_eq!(project.read("b.txt"), "# from b");
}

// ---------------------------------------------------------------------------
// Asset Library dependencies
// ---------------------------------------------------------------------------

/// Mount an asset-library scenario: the detail endpoint for `asset_id` plus
/// the downloadable archive. [`AssetDetailBody::new_with_file`] lets the mock
/// own the download URL and serve the zip at its own per-asset route, so the
/// caller only supplies the asset metadata and the archive's files. Asset-lib
/// deps default to `strip_components = 1`, so the zip carries its files under
/// one wrapper directory.
async fn mount_asset(api: &MockApi, asset_id: u32, files: &[(&str, &str)]) {
    api.mount_asset_detail(
        asset_id,
        AssetDetailBody::new_with_file(
            asset_id,
            "Test Addon",
            "Author",
            "MIT",
            1,
            "1.0.0",
            "https://example.com/test-addon",
            zip_bytes(files),
        ),
    )
    .await;
}

#[tokio::test]
async fn sync_asset_lib_installs() {
    // An asset-lib dependency resolves through the asset library detail
    // endpoint, downloads the reported zip, and installs (strip 1) like an
    // archive dep, recording url + asset_version in the lock file.
    let api = MockApi::start().await;
    mount_asset(&api, 42, &[("addon/addon-file.txt", "addon content")]).await;

    let mut project = TestProject::new();
    project.env_api(&api).config().asset("my-addon", 42).write();

    project
        .cmd()
        .arg("sync")
        .assert()
        .success()
        .stdout(contains("my-addon"));

    assert_eq!(project.read("addon-file.txt"), "addon content");
    assert!(project.exists("ggg.lock"));

    let lock = project.read("ggg.lock");
    assert!(lock.contains("name = \"my-addon\""));
    assert!(lock.contains("asset_id = 42"));
    assert!(lock.contains("asset_version"));
}

#[tokio::test]
async fn sync_asset_lib_modified_owned_blocks_then_force_overwrites() {
    // Asset-lib deps must obey the same ownership rules as git/archive deps:
    // editing an installed file blocks re-sync, and `--force` restores it.
    let api = MockApi::start().await;
    mount_asset(&api, 42, &[("addon/addon-file.txt", "addon content")]).await;

    let mut project = TestProject::new();
    project.env_api(&api).config().asset("my-addon", 42).write();

    project.cmd().arg("sync").assert().success();
    project.write("addon-file.txt", "# user change");

    // Modified owned file: blocked without --force.
    project
        .cmd()
        .arg("sync")
        .assert()
        .failure()
        .stderr(contains("modified since last install"));

    // --force discards the edit and restores the installed content.
    project.cmd().args(["sync", "--force"]).assert().success();

    assert_eq!(project.read("addon-file.txt"), "addon content");
}

#[tokio::test]
async fn sync_asset_lib_remap_removes_stale_files() {
    // Remapping an asset-lib dep to a different asset id must clean up files
    // from the old id (same parity as the git/archive remap tests).
    let api = MockApi::start().await;
    mount_asset(&api, 42, &[("a/a-file.txt", "# from 42")]).await;
    mount_asset(&api, 43, &[("b/b-file.txt", "# from 43")]).await;

    let mut project = TestProject::new();
    project.env_api(&api).config().asset("my-addon", 42).write();

    project.cmd().arg("sync").assert().success();
    assert_eq!(project.read("a-file.txt"), "# from 42");

    // Repoint the same dep at a different asset id.
    project.env_api(&api).config().asset("my-addon", 43).write();

    project.cmd().arg("sync").assert().success();

    // The new id's file is installed and the old id's file is cleaned up.
    assert_eq!(project.read("b-file.txt"), "# from 43");
    assert!(
        !project.exists("a-file.txt"),
        "stale file should be removed after asset id remap"
    );
}

// ---------------------------------------------------------------------------
// Godot engine download (AC: MockApi mounts builds API + fake zip; sync
// downloads and installs the engine end-to-end with no real network)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sync_downloads_engine_end_to_end() {
    // With an empty Godot cache, `ggg sync` must query the mocked GitHub
    // builds API, select the current platform's asset by name, download and
    // extract the fake engine zip, and install it into the isolated cache --
    // all over the hermetic wiremock network.
    let api = MockApi::start().await;
    let release: GodotRelease = "4.3-stable".parse().unwrap();
    api.mount_godot_release(&release).await;

    let mut project = TestProject::new();
    project.env_builds_api(&api);
    project.config().write_no_seed();

    project.cmd().arg("sync").assert().success();

    // The engine was downloaded and extracted into the isolated cache,
    // under the release's cache key directory.
    let release_dir = project.cache_dir().join("godot").join("4.3-stable");
    assert!(
        release_dir.is_dir(),
        "engine should be cached at {}",
        release_dir.display()
    );

    // The extracted archive contains a Godot executable for this platform.
    let files: Vec<String> = std::fs::read_dir(&release_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        files.iter().any(|f| f.to_lowercase().starts_with("godot")),
        "expected a Godot executable in cache, got: {files:?}"
    );

    // The builds API endpoint was actually queried, not short-circuited.
    let requests = api.received_requests().await;
    let paths: Vec<String> = requests.iter().map(|r| r.url.path().to_string()).collect();
    assert!(
        paths.iter().any(|p| p == "/releases/tags/4.3-stable"),
        "expected a builds API request, got: {paths:?}"
    );
}

#[tokio::test]
async fn sync_downloads_engine_from_mocked_release_with_exact_asset() {
    // Lower-level variant proving the release's asset list is parsed: mount
    // the builds API with a fake platform zip and more assets than the one
    // ggg picks, then assert ggg resolves and installs the right one. This
    // exercises `select_asset` against a realistic (non-single-asset) release.
    let api = MockApi::start().await;
    let release: GodotRelease = "4.3-stable".parse().unwrap();
    let (plat_name, plat_zip) = fake_godot_asset(&release);
    let mut assets = vec![
        (
            "Godot_v4.3-stable_export_templates.tpz".to_string(),
            fake_template_tpz(&release),
        ),
        (
            "Godot_v4.3-stable_unrelated.txt".to_string(),
            b"irrelevant".to_vec(),
        ),
    ];
    assets.push((plat_name, plat_zip));
    api.mount_builds_api("4.3-stable", assets).await;

    let mut project = TestProject::new();
    project.env_builds_api(&api);
    project.config().write_no_seed();

    project.cmd().arg("sync").assert().success();

    let release_dir = project.cache_dir().join("godot").join("4.3-stable");
    assert!(
        release_dir.is_dir(),
        "engine should be cached at {}",
        release_dir.display()
    );
}

// ---------------------------------------------------------------------------
// Export templates download (AC: works via GGG_GODOT_DOWNLOADS_BASE)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sync_downloads_export_templates() {
    // With export_templates = true and an empty cache, `ggg sync` must boot
    // the engine from the mocked builds API, download the export-templates
    // `.tpz` from the mocked downloads base, and install it into the Godot
    // data dir (redirected to a temp dir via GGG_GODOT_DATA_DIR).
    let data_dir = tempfile::tempdir().unwrap();
    let api = MockApi::start().await;
    let release: GodotRelease = "4.3-stable".parse().unwrap();
    api.mount_godot_release(&release).await;
    api.mount_export_templates(fake_template_tpz(&release))
        .await;

    let mut project = TestProject::new();
    project
        .env_builds_api(&api)
        .env_downloads_base(&api)
        .env_data_dir(data_dir.path());
    project.config().export_templates(true).write_no_seed();

    project.cmd().arg("sync").assert().success();

    // version.txt is the marker Godot's own template manager recognises.
    let godot_subdir = if cfg!(target_os = "linux") {
        "godot"
    } else {
        "Godot"
    };
    let version_txt = data_dir
        .path()
        .join(godot_subdir)
        .join("export_templates")
        .join("4.3.stable")
        .join("version.txt");
    assert!(
        version_txt.exists(),
        "export templates should be installed at {}",
        version_txt.display()
    );

    // The downloads base endpoint received a request for the .tpz.
    let requests = api.received_requests().await;
    let queries: Vec<String> = requests
        .iter()
        .map(|r| r.url.query().unwrap_or_default().to_string())
        .collect();
    assert!(
        queries
            .iter()
            .any(|q| q.contains("slug=export_templates.tpz")),
        "expected a downloads-host request for the .tpz, got: {queries:?}"
    );
}
