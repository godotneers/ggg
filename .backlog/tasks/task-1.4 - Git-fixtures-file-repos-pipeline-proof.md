---
id: TASK-1.4
title: 'Git fixtures: file:// repos + pipeline proof'
status: Done
assignee: []
created_date: '2026-09-08 05:27'
updated_date: '2026-09-11 05:11'
labels: []
dependencies:
  - TASK-1.2
modified_files:
  - tests/common/git_fixtures.rs
  - tests/common/mod.rs
  - tests/git_fixtures.rs
parent_task_id: TASK-1
type: task
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
De-risk the Windows file:// question early. New tests/common/git_fixtures.rs: build a local bare repo with gix (blob, commit, refs/heads/main, refs/tags/v1.0.0), expose the repo path plus a file:// URL builder.
Spike test: a git dep in ggg.toml pointing at file://, run ggg sync through the binary (seeding the Godot cache as needed), assert install + ggg.lock.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 git_fixtures builds a local bare repo with a blob, a commit, refs/heads/main and refs/tags/v1.0.0
- [x] #2 Fixture exposes repo path and file:// URL builder
- [x] #3 One spike test runs sync on a file:// git dep and asserts install + ggg.lock
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. tests/common/git_fixtures.rs: BareRepo uses the git binary (assumed on dev machines) - worktree git init, write plugin.gd, add/commit (fixed identity), branch -M main, clone --bare, tag v1.0.0 in bare repo. Exposes repo_path(), sha(), file_url() (file:///C:/... on Win, file:///... on Unix).
2. tests/common/mod.rs: register pub mod git_fixtures, add seed_godot_cache(cache_root, release) writing a dummy godot*.exe into GGG_CACHE_DIR/godot/<cache_key>.
3. tests/git_fixtures.rs: spike - build BareRepo, TestProject with GGG_CACHE_DIR tempdir + seeded Godot cache, ggg.toml git dep at file:// rev=main and a second test rev=v1.0.0, run ggg sync, assert success + installed plugin.gd + ggg.lock SHA/rev.
4. Verify cargo test/clippy/fmt --check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Design decision: use the git binary instead of gix object-writing. The test suite runs on developer machines that already have git (the project is checked out with it); end users never run integration tests. This is far simpler than programmatically writing blobs/trees/commits/refs with the gix API. Initial attempt parsed the canonicalized \\?\ extended path as an invalid URL - fixed by building file:// from the raw path with forward slashes.

Post-review refactor (addresses reviewer comments):
- TestProject now owns a default GGG_CACHE_DIR tempdir and sets it in new() so tests never set it manually.
- seed_godot_cache became a private TestProject member auto-invoked by ConfigBuilder::write() using the pinned godot version, removing manual seeding.
- TestProject::path() replaced with read(rel) + exists(rel) helpers.
- BareRepo stores a single bare_dir TempDir (verified git clone --bare into an existing empty tempdir clones directly and carries refs/heads/main + HEAD), removing the unused _worktree and _bare_parent fields.
- Inlined file_url() and merged run_git/run_git_output into one run_git returning trimmed stdout.

All verified: cargo test (all pass), clippy --all-targets clean, fmt --check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
- Added tests/common/git_fixtures.rs: BareRepo built with the git binary (worktree init -> add plugin.gd -> commit -> branch -M main -> clone --bare -> tag v1.0.0); exposes repo_path/sha/file_url.
- Registered pub mod git_fixtures and added seed_godot_cache() in tests/common/mod.rs.
- Added two spike tests in tests/git_fixtures.rs proving the full sync pipeline (resolve -> fetch -> cache -> install -> ggg.lock) over the local file:// transport for both a branch rev (main) and a tag rev (v1.0.0).
- Verified: cargo test (all 232 pass), cargo clippy --all-targets clean, cargo fmt --check clean.
<!-- SECTION:FINAL_SUMMARY:END -->
