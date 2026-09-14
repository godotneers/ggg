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

/// A git dependency whose repository contains a submodule entry installs
/// successfully, matching `git archive` semantics: the submodule's commit is
/// never fetched from the superproject, so it produces no file and no
/// directory in the installed output.
#[test]
fn sync_succeeds_with_submodule_entry() {
    let repo = BareRepo::builder()
        .with("addon/plugin.gd", "# addon file")
        // A submodule whose commit OID exists only in the (absent) submodule
        // repository - the failure mode from the bug report, where extraction
        // tried to look the OID up in the parent and hit "failed to find blob".
        .with_submodule("addon/native/godot-cpp", "a".repeat(40))
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

    // The real file is installed...
    assert_eq!(project.read("addon/plugin.gd"), "# addon file");

    // ...and the submodule path produces neither a file nor a directory.
    assert!(!project.exists("addon/native"));
}
