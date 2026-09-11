//! End-to-end spike: install a git-sourced dependency from a local `file://`
//! repository via the full `ggg sync` pipeline.
//!
//! This proves the dependency pipeline (resolve -> fetch -> cache -> install
//! -> lock file) works against a real bare git repository over the local
//! `file://` transport, without needing a network or a daemon.

mod common;

use predicates::str::contains;

use common::TestProject;
use common::git_fixtures::BareRepo;

/// A git dependency pointing at a `file://` repo resolves, downloads, installs,
/// and is recorded in `ggg.lock` after `ggg sync`.
#[test]
fn sync_installs_git_dependency_from_file_url() {
    let repo = BareRepo::builder()
        .with("plugin.gd", "# hello from the fixture")
        .build();

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

    // The dependency's file is installed at the project root (no map, no
    // strip_components), with the exact contents from the repository.
    assert_eq!(project.read("plugin.gd"), "# hello from the fixture");

    // ggg.lock records the dependency with the resolved SHA.
    let lock_str = project.read("ggg.lock");
    assert!(lock_str.contains("name = \"my-addon\""));
    assert!(lock_str.contains("git = \""));
    assert!(lock_str.contains(&format!("sha = \"{}\"", repo.sha())));
    assert!(lock_str.contains("rev = \"main\""));
}

/// A `rev` that points at a tag resolves to the tagged commit's SHA.
#[test]
fn sync_resolves_tag_rev() {
    let repo = BareRepo::builder().with("plugin.gd", "# v1.0.0").build();

    let project = TestProject::new();
    project
        .config()
        .git("tagged-addon", repo.file_url(), "v1.0.0")
        .write();

    project
        .cmd()
        .arg("sync")
        .assert()
        .success()
        .stdout(contains("tagged-addon"));

    let lock_str = project.read("ggg.lock");
    assert!(lock_str.contains("rev = \"v1.0.0\""));
    assert!(lock_str.contains(&format!("sha = \"{}\"", repo.sha())));
}
