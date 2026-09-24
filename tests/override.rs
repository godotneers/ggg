//! End-to-end tests for the Godot executable override (`--godot` flag and
//! `GGG_GODOT_EXECUTABLE` env var).
//!
//! The override lets a user supply their own Godot binary instead of the
//! managed download. These tests prove the launch path (the no-op binary
//! compiled at test time), the guardrail (an invalid path is a clear error),
//! the precedence (`--godot` > env var > managed default), and that an
//! override performs no managed download.

mod common;

use predicates::str::contains;

use common::TestProject;
use common::noop_executable;
use ggg::envvars::GODOT_EXECUTABLE_ENV_VAR;

/// A path within `dir` that is guaranteed not to exist yet.
fn missing_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("missing-godot.exe")
}

/// Assert that `ggg` never touched the managed engine cache: the `godot/`
/// subdirectory must be absent.
fn assert_no_engine_cached(project: &TestProject) {
    let godot_dir = project.cache_dir().join("godot");
    assert!(
        !godot_dir.exists(),
        "override should skip the managed engine download, got {}",
        godot_dir.display()
    );
}

// ---------------------------------------------------------------------------
// Positive launch through the --godot flag (both run and edit)
// ---------------------------------------------------------------------------

#[test]
fn run_godot_flag_launches_override_without_downloading() {
    let exe = noop_executable();
    let project = TestProject::new();
    project.config().write_no_seed();

    project
        .cmd()
        .args(["run", "--godot"])
        .arg(&exe)
        .assert()
        .success();

    assert_no_engine_cached(&project);
}

#[test]
fn edit_godot_flag_launches_override_without_downloading() {
    let exe = noop_executable();
    let project = TestProject::new();
    project.config().write_no_seed();

    // `ggg edit` forwards `--editor .`; the no-op binary must ignore it.
    project
        .cmd()
        .args(["edit", "--godot"])
        .arg(&exe)
        .assert()
        .success();

    assert_no_engine_cached(&project);
}

// ---------------------------------------------------------------------------
// Positive launch through GGG_GODOT_EXECUTABLE (both run and edit)
// ---------------------------------------------------------------------------

#[test]
fn run_env_var_launches_override_without_downloading() {
    let exe = noop_executable();
    let mut project = TestProject::new();
    project.env(GODOT_EXECUTABLE_ENV_VAR, exe.display().to_string());
    project.config().write_no_seed();

    project.cmd().arg("run").assert().success();

    assert_no_engine_cached(&project);
}

#[test]
fn edit_env_var_launches_override_without_downloading() {
    let exe = noop_executable();
    let mut project = TestProject::new();
    project.env(GODOT_EXECUTABLE_ENV_VAR, exe.display().to_string());
    project.config().write_no_seed();

    project.cmd().arg("edit").assert().success();

    assert_no_engine_cached(&project);
}

// ---------------------------------------------------------------------------
// Guardrail: an invalid override path is a clear error, not a download
// ---------------------------------------------------------------------------

#[test]
fn run_godot_flag_missing_path_errors_clearly() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = missing_path(tmp.path());
    let project = TestProject::new();
    project.config().write_no_seed();

    project
        .cmd()
        .args(["run", "--godot"])
        .arg(&missing)
        .assert()
        .failure()
        .stderr(contains("does not exist"))
        .stderr(contains("--godot"))
        .stderr(contains(missing.display().to_string()));

    // Guardrail fires before any download attempt.
    assert_no_engine_cached(&project);
}

#[test]
fn run_godot_flag_directory_path_errors_clearly() {
    let dir = tempfile::tempdir().unwrap();
    let project = TestProject::new();
    project.config().write_no_seed();

    project
        .cmd()
        .args(["run", "--godot"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(contains("not a file"))
        .stderr(contains("--godot"));

    assert_no_engine_cached(&project);
}

#[test]
fn run_env_var_missing_path_errors_clearly() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = missing_path(tmp.path());
    let mut project = TestProject::new();
    project.env(GODOT_EXECUTABLE_ENV_VAR, missing.display().to_string());
    project.config().write_no_seed();

    project
        .cmd()
        .arg("run")
        .assert()
        .failure()
        .stderr(contains("does not exist"))
        .stderr(contains("GGG_GODOT_EXECUTABLE"))
        .stderr(contains(missing.display().to_string()));

    assert_no_engine_cached(&project);
}

// ---------------------------------------------------------------------------
// Precedence: --godot > GGG_GODOT_EXECUTABLE > managed default
// ---------------------------------------------------------------------------

#[test]
fn godot_flag_wins_over_invalid_env_var() {
    let exe = noop_executable();
    let tmp = tempfile::tempdir().unwrap();
    let missing = missing_path(tmp.path());
    let mut project = TestProject::new();
    project.env(GODOT_EXECUTABLE_ENV_VAR, missing.display().to_string());
    project.config().write_no_seed();

    // The env var points at a missing binary, but --godot is valid and wins.
    project
        .cmd()
        .args(["run", "--godot"])
        .arg(&exe)
        .assert()
        .success();

    assert_no_engine_cached(&project);
}

#[test]
fn godot_flag_overrides_valid_env_var() {
    let exe = noop_executable();
    let tmp = tempfile::tempdir().unwrap();
    let missing = missing_path(tmp.path());
    let mut project = TestProject::new();
    project.env(GODOT_EXECUTABLE_ENV_VAR, exe.display().to_string());
    project.config().write_no_seed();

    // The env var is valid, but the bad --godot flag wins and must error on
    // the flag's path rather than silently using the env var.
    project
        .cmd()
        .args(["run", "--godot"])
        .arg(&missing)
        .assert()
        .failure()
        .stderr(contains("does not exist"))
        .stderr(contains("--godot"));

    assert_no_engine_cached(&project);
}

// ---------------------------------------------------------------------------
// ggg sync honours the env var and skips the managed download
// ---------------------------------------------------------------------------

#[test]
fn sync_env_var_skips_managed_download() {
    let exe = noop_executable();
    let mut project = TestProject::new();
    project.env(GODOT_EXECUTABLE_ENV_VAR, exe.display().to_string());
    project.config().write_no_seed();

    project.cmd().arg("sync").assert().success();

    assert_no_engine_cached(&project);
    assert!(project.exists("ggg.lock"));
}

#[test]
fn sync_godot_flag_skips_managed_download() {
    let exe = noop_executable();
    let project = TestProject::new();
    project.config().write_no_seed();

    project
        .cmd()
        .args(["sync", "--godot"])
        .arg(&exe)
        .assert()
        .success();

    assert_no_engine_cached(&project);
    assert!(project.exists("ggg.lock"));
}

#[test]
fn sync_godot_flag_missing_path_errors_clearly() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = missing_path(tmp.path());
    let project = TestProject::new();
    project.config().write_no_seed();

    project
        .cmd()
        .args(["sync", "--godot"])
        .arg(&missing)
        .assert()
        .failure()
        .stderr(contains("does not exist"))
        .stderr(contains("--godot"))
        .stderr(contains(missing.display().to_string()));

    // Guardrail fires before any download attempt.
    assert_no_engine_cached(&project);
}

// ---------------------------------------------------------------------------
// Sanity: --godot is parsed even though run/edit tail forwards args
// ---------------------------------------------------------------------------

#[test]
fn run_godot_flag_accepted_in_help() {
    let project = TestProject::new();
    project
        .cmd()
        .args(["run", "--help"])
        .assert()
        .success()
        .stdout(contains("--godot"));
}
