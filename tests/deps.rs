//! Integration tests for `ggg deps`.
//!
//! `ggg deps` is offline and non-interactive: it reads `ggg.toml` from the
//! current directory and prints a table of the declared dependencies.

mod common;

use predicates::str::contains;

use common::TestProject;

/// Without a `ggg.toml`, the command fails with a hint to run `ggg init`.
#[test]
fn deps_fails_when_no_ggg_toml() {
    let project = TestProject::new();

    project
        .cmd()
        .arg("deps")
        .assert()
        .failure()
        .stderr(contains("no ggg.toml found in the current directory"));
}

/// An empty dependency list prints a friendly message and succeeds.
#[test]
fn deps_shows_nothing_when_no_dependencies() {
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .arg("deps")
        .assert()
        .success()
        .stdout(contains("No dependencies in ggg.toml."));
}

/// Each declared dependency appears in the table with its source info.
#[test]
fn deps_lists_dependencies() {
    let project = TestProject::new();
    project
        .config()
        .git("dep1", "https://example.com/dep1", "v1.2.3")
        .archive("addon", "https://example.com/addon.zip")
        .asset_lib("dialogic", 1216)
        .write();

    let assert = project.cmd().arg("deps").assert().success();
    assert
        .stdout(contains("dep1"))
        .stdout(contains("v1.2.3"))
        .stdout(contains("addon"))
        .stdout(contains("addon.zip"))
        .stdout(contains("dialogic"))
        .stdout(contains("asset-lib"))
        .stdout(contains("asset #1216"));
}

/// Asset Store dependencies render with type `asset-store` and the
/// `publisher_slug/asset_slug:version` spec.
#[test]
fn deps_lists_asset_store_dependency() {
    let project = TestProject::new();
    project
        .config()
        .asset_store("my-addon", "publisher/my-addon:1.2.3")
        .write();

    project
        .cmd()
        .arg("deps")
        .assert()
        .success()
        .stdout(contains("my-addon"))
        .stdout(contains("asset-store"))
        .stdout(contains("publisher/my-addon:1.2.3"));
}
