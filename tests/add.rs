//! Integration tests for `ggg add`.
//!
//! Covers the non-interactive add paths: archive, git, and asset (via --id/-y).
//! Interactive disambiguation (0/1/2-5/6+ search results) is permanently out
//! of scope — driving dialoguer from a test harness is fragile.

mod common;

use predicates::str::contains;

use common::TestProject;
use common::archive::zip_bytes;
use common::git_fixtures::BareRepo;
use common::wiremock::{AssetDetailBody, MockApi};

// ---------------------------------------------------------------------------
// AC #1: add archive writes the dep with no network
// ---------------------------------------------------------------------------

#[test]
fn add_archive_writes_dep() {
    // `ggg add archive` must write a new dependency entry to ggg.toml
    // without contacting any remote.  The URL and name are stored exactly
    // as given; downloading and SHA verification happen later during sync.
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args([
            "add",
            "archive",
            "http://example.com/addon.zip",
            "--name",
            "my-addon",
        ])
        .assert()
        .success()
        .stdout(contains("Added \"my-addon\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    assert_eq!(config.dependency[0].name, "my-addon");
    assert_eq!(
        config.dependency[0].url.as_deref(),
        Some("http://example.com/addon.zip")
    );
}

// ---------------------------------------------------------------------------
// AC #2: add archive rejects a non-.zip/.tar.gz/.tgz URL
// ---------------------------------------------------------------------------

#[test]
fn add_archive_rejects_bad_extension() {
    // Only .zip, .tar.gz, and .tgz archives are supported.  Any other
    // extension must be rejected up front with a clear error rather than
    // silently storing a URL that sync cannot handle.
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args([
            "add",
            "archive",
            "http://example.com/file.rar",
            "--name",
            "my-rar",
        ])
        .assert()
        .failure()
        .stderr(contains("unrecognised archive format"));
}

// ---------------------------------------------------------------------------
// AC #3: bare add <url>.zip routes to the archive path
// ---------------------------------------------------------------------------

#[test]
fn bare_add_zip_routes_to_archive() {
    // The convenience form `ggg add <url>` auto-detects the dependency type
    // from the input.  A URL ending in .zip must be routed to the archive
    // handler, producing the same result as `ggg add archive <url>`.
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args(["add", "http://example.com/addon.zip", "--name", "my-addon"])
        .assert()
        .success()
        .stdout(contains("Added \"my-addon\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    assert_eq!(config.dependency[0].name, "my-addon");
    assert_eq!(
        config.dependency[0].url.as_deref(),
        Some("http://example.com/addon.zip")
    );
}

// ---------------------------------------------------------------------------
// AC #4: fails with 'no ggg.toml found'
// ---------------------------------------------------------------------------

#[test]
fn add_fails_without_ggg_toml() {
    // Without a ggg.toml there is no Godot version or dependency list to
    // work with, so the command must fail immediately with a hint to run
    // `ggg init` rather than guess or create a default config.
    let project = TestProject::new();

    project
        .cmd()
        .args([
            "add",
            "archive",
            "http://example.com/addon.zip",
            "--name",
            "x",
        ])
        .assert()
        .failure()
        .stderr(contains("no ggg.toml found"));
}

// ---------------------------------------------------------------------------
// AC #5: duplicate name rejected
// ---------------------------------------------------------------------------

#[test]
fn add_rejects_duplicate_name() {
    // Every dependency name must be unique within ggg.toml.  Adding a
    // dependency whose name collides with an existing one must fail with
    // a clear error rather than silently overwriting or creating a
    // duplicate entry.
    let project = TestProject::new();
    project
        .config()
        .archive("existing", "http://example.com/first.zip")
        .write();

    project
        .cmd()
        .args([
            "add",
            "archive",
            "http://example.com/second.zip",
            "--name",
            "existing",
        ])
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

// ---------------------------------------------------------------------------
// AC #6: add with no arguments fails
// ---------------------------------------------------------------------------

#[test]
fn add_fails_with_no_args() {
    // Running `ggg add` with no arguments is ambiguous — the tool cannot
    // know whether the user intends a git, archive, or asset dependency.
    // It must fail with an error explaining the expected usage.
    let project = TestProject::new();
    project.config().write();

    project.cmd().arg("add").assert().failure();
}

// ---------------------------------------------------------------------------
// AC #7: add git <file://url>@rev --name -y resolves and writes the dep
// ---------------------------------------------------------------------------

#[test]
fn add_git_resolves_and_writes() {
    // `ggg add git` must resolve the given rev (branch, tag, or SHA)
    // against the remote and write both the dependency to ggg.toml.
    // Using a local file:// bare repo keeps the test hermetic while
    // exercising the real git resolution path through gix.  The -y flag
    // skips interactive prompts, and --name overrides the inferred name.
    let repo = BareRepo::builder()
        .with("plugin.gd", "# hello from the fixture")
        .build();

    let project = TestProject::new();
    project.config().write();

    let url_with_rev = format!("{}@main", repo.file_url());
    project
        .cmd()
        .args(["add", "git", &url_with_rev, "--name", "my-addon", "-y"])
        .assert()
        .success()
        .stdout(contains("Added \"my-addon\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    assert_eq!(config.dependency[0].name, "my-addon");
    assert_eq!(
        config.dependency[0].git.as_deref(),
        Some(repo.file_url().as_str())
    );
    assert_eq!(config.dependency[0].rev.as_deref(), Some("main"));
}

// ---------------------------------------------------------------------------
// AC #8: add asset --id N --name -y fetches and writes the dep via wiremock
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_asset_via_id_writes_dep() {
    // `ggg add asset --id N` fetches the asset metadata from the Godot
    // Asset Library API and writes the dependency to ggg.toml.  The test
    // wires the API endpoint to a wiremock server so no real network is
    // needed.  The -y flag skips the confirmation prompt, and --name
    // overrides the inferred name.
    let api = MockApi::start().await;
    api.mount_asset_detail(
        42,
        AssetDetailBody::new_with_file(
            42,
            "Test Addon",
            "Author",
            "MIT",
            1,
            "1.0.0",
            "https://example.com/test-addon",
            zip_bytes(&[("addon/addon-file.txt", "addon content")]),
        ),
    )
    .await;

    let mut project = TestProject::new();
    project.env_api(&api).config().write();

    project
        .cmd()
        .args(["add", "asset", "--id", "42", "--name", "test-addon", "-y"])
        .assert()
        .success()
        .stdout(contains("Added \"test-addon\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    assert_eq!(config.dependency[0].name, "test-addon");
    assert_eq!(config.dependency[0].asset_id, Some(42));
}

// ---------------------------------------------------------------------------
// AC #10: Interactive add asset <query> disambiguation is permanently out of
// scope; only the non-interactive --id/-y path is tested.
// ---------------------------------------------------------------------------
