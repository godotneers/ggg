//! Smoke tests for the CLI surface: help, version, and argument parsing.
//!
//! These exercise clap only, so no fixtures or network are needed.

mod common;

use predicates::str::contains;

use common::TestProject;

#[test]
fn help_shows_subcommands() {
    TestProject::new()
        .cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("A project manager for Godot games"))
        .stdout(contains("sync"))
        .stdout(contains("search"));
}

#[test]
fn short_help_works() {
    TestProject::new()
        .cmd()
        .arg("-h")
        .assert()
        .success()
        .stdout(contains("Usage: ggg"));
}

#[test]
fn version_prints_version() {
    TestProject::new()
        .cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn requires_a_subcommand() {
    TestProject::new()
        .cmd()
        .assert()
        .failure()
        .stderr(contains("Usage: ggg"));
}

#[test]
fn unknown_subcommand_fails() {
    TestProject::new()
        .cmd()
        .arg("frobnicate")
        .assert()
        .failure()
        .stderr(contains("unrecognized subcommand"));
}

#[test]
fn unknown_flag_fails() {
    TestProject::new()
        .cmd()
        .args(["deps", "--bogus"])
        .assert()
        .failure()
        .stderr(contains("unexpected argument"));
}
