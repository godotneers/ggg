//! Integration tests for `ggg diff`.
//!
//! `ggg diff` compares the installed (cached) version of every ggg-owned file
//! with what is currently on disk, and prints a unified diff for any that have
//! been locally modified. It exits 0 when nothing is modified and 1 when any
//! diff is shown, so scripts can use the exit status as a signal.
//!
//! Git dependencies are served from local `file://` bare repositories (see
//! `common::git_fixtures`), so these tests need no network.

mod common;

use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

use common::TestProject;
use common::git_fixtures::BareRepo;

// ---------------------------------------------------------------------------
// AC #1: fails with 'no ggg.toml found' when no config exists
// ---------------------------------------------------------------------------

#[test]
fn diff_fails_without_ggg_toml() {
    // With no ggg.toml there is nothing to diff, so the command must refuse
    // and point the user at `ggg init` rather than guess.
    let project = TestProject::new();

    project
        .cmd()
        .arg("diff")
        .assert()
        .failure()
        .stderr(contains("no ggg.toml found"));
}

// ---------------------------------------------------------------------------
// AC #2: clean project exits 0 with 'no modified files'
// ---------------------------------------------------------------------------

#[test]
fn diff_clean_project_exits_zero() {
    // An installed dependency whose files are untouched has nothing to report.
    // `ggg diff` succeeds (exit 0) and says so explicitly.
    let repo = BareRepo::builder().with("plugin.gd", "# hello").build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    project.cmd().arg("sync").assert().success();

    project
        .cmd()
        .arg("diff")
        .assert()
        .success()
        .stderr(contains("no modified files"));
}

// ---------------------------------------------------------------------------
// AC #3: modified owned file exits 1 and prints a unified diff
// ---------------------------------------------------------------------------

#[test]
fn diff_modified_file_shows_unified_diff() {
    // Once a ggg-owned file is edited, `ggg diff` prints a unified diff of the
    // change and exits 1 so callers know files were modified.
    let repo = BareRepo::builder().with("plugin.gd", "# hello").build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    project.cmd().arg("sync").assert().success();
    project.write("plugin.gd", "# user modification");

    project
        .cmd()
        .arg("diff")
        .assert()
        .failure()
        .stdout(contains("@@"));
}

// ---------------------------------------------------------------------------
// AC #4: diff <file> filters to a single file
// ---------------------------------------------------------------------------

#[test]
fn diff_file_filter_limits_to_single_file() {
    // Passing a path restricts the report to that file: the other modified
    // file is neither advertised nor diffed.
    let repo = BareRepo::builder()
        .with("a.txt", "# from a")
        .with("b.txt", "# from b")
        .build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    project.cmd().arg("sync").assert().success();
    project.write("a.txt", "# a edited");
    project.write("b.txt", "# b edited");

    project
        .cmd()
        .args(["diff", "a.txt"])
        .assert()
        .failure()
        .stdout(contains("a.txt"))
        .stdout(contains("b.txt").not());
}

// ---------------------------------------------------------------------------
// AC #5: binary file is skipped with a note
// ---------------------------------------------------------------------------

#[test]
fn diff_binary_file_is_skipped() {
    // Non-UTF-8 files cannot be diffed line-by-line, so they are skipped with
    // an explanatory note. The skip still counts as output, so the command
    // exits 1 (files were modified).
    let repo = BareRepo::builder()
        .with("bin.dat", &[0x00, 0x01, 0xff][..])
        .build();
    let project = TestProject::new();
    project
        .config()
        .git("my-addon", repo.file_url(), "main")
        .write();

    project.cmd().arg("sync").assert().success();
    project.write("bin.dat", &[0xfe, 0xff, 0x00][..]);

    project
        .cmd()
        .arg("diff")
        .assert()
        .failure()
        .stdout(contains("(binary file, skipping)"));
}
