//! Integration tests for `ggg remove`.
//!
//! `ggg remove` is offline and non-interactive: it removes a named dependency
//! from `ggg.toml`. No files are touched on disk at this point.

mod common;

use predicates::str::contains;

use common::TestProject;

/// The named dependency is removed from `ggg.toml`.
#[test]
fn remove_removes_named_dependency() {
    let project = TestProject::new();
    project
        .config()
        .git("dep1", "https://example.com/dep1", "v1.2.3")
        .git("dep2", "https://example.com/dep2", "v1.2.3")
        .write();

    project
        .cmd()
        .args(["remove", "dep1"])
        .assert()
        .success()
        .stdout(contains("Removed \"dep1\" from ggg.toml."));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    assert_eq!(config.dependency[0].name, "dep2");
}

/// Removing one dependency leaves the others untouched.
#[test]
fn remove_keeps_other_dependencies() {
    let project = TestProject::new();
    project
        .config()
        .git("dep1", "https://example.com/dep1", "v1.2.3")
        .git("dep2", "https://example.com/dep2", "v1.2.3")
        .write();

    project.cmd().args(["remove", "dep2"]).assert().success();

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    assert_eq!(config.dependency[0].name, "dep1");
}

/// Removing the last dependency leaves an empty list.
#[test]
fn remove_last_dependency_leaves_empty_list() {
    let project = TestProject::new();
    project
        .config()
        .git("dep1", "https://example.com/dep1", "v1.2.3")
        .write();

    project.cmd().args(["remove", "dep1"]).assert().success();

    let config = project.read_ggg_toml();
    assert!(config.dependency.is_empty());
}

/// An unknown dependency name is reported as an error.
#[test]
fn remove_fails_for_unknown_dependency() {
    let project = TestProject::new();
    project
        .config()
        .git("dep1", "https://example.com/dep1", "v1.2.3")
        .git("dep2", "https://example.com/dep2", "v1.2.3")
        .write();

    project
        .cmd()
        .args(["remove", "nonexistent"])
        .assert()
        .failure()
        .stderr(contains(
            "no dependency named \"nonexistent\" found in ggg.toml",
        ));
}

/// Without a `ggg.toml`, the command fails with a hint to run `ggg init`.
#[test]
fn remove_fails_when_no_ggg_toml() {
    let project = TestProject::new();

    project
        .cmd()
        .args(["remove", "gut"])
        .assert()
        .failure()
        .stderr(contains("no ggg.toml found in the current directory"));
}
