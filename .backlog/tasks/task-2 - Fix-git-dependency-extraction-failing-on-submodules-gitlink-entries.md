---
id: TASK-2
title: Fix git dependency extraction failing on submodules (gitlink entries)
status: Done
assignee: []
created_date: '2026-09-11 06:17'
updated_date: '2026-09-14 04:50'
labels: []
dependencies: []
references:
  - 'https://github.com/godotneers/ggg/issues/3'
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`ggg sync` / `ggg ls-dep` fails on a git dependency whose repository contains a Git submodule. Reported at https://github.com/godotneers/ggg/issues/3 (ggg 0.4.0): dependency fetches fine, then extraction errors with "failed to find blob <sha>". Repro: dependency `git = "https://github.com/JetBrains/godot-support.git", rev = "master"`. Its single submodule is the gitlink `godot-editor-addon/native/rider-plugin/godot-cpp` (mode 160000, resolves to godotengine/godot-cpp) rendering as `160000 commit <sha>` in `git ls-tree`; the failing object is that gitlink commit OID.

Root cause (confirmed in src/dependency/cache.rs, HEAD identical to v0.4.0): in `extract_tree()` the "skip non-blob entries" filter classifies `gix::index::entry::Mode` (a bitflags set whose constants are raw octal git modes) with `contains()`. `Mode::COMMIT` = 0o160000 shares bits with `Mode::SYMLINK` = 0o120000 (`0o160000 & 0o120000 == 0o120000`), so `COMMIT.contains(SYMLINK)` is true and a gitlink entry passes the filter, reaches `write_blob()`, and `repo.find_object(entry.id)` fails because the gitlink commit object lives in the submodule repo, not the parent.

Decision: skip gitlink/submodule entries silently (no warning), matching `git archive` / GitHub tarball semantics. Verified against git source: `git archive` never warns and never fetches submodule contents (it only emits an empty-dir placeholder; ggg should emit nothing at all). Skipping does not break the issue repro: the godot-support submodule is godot-cpp C++ source used only to build the native Rider GDExtension, which is distributed prebuilt via releases; the installable Godot editor addon tree is unaffected.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A git dependency whose tree contains a gitlink (submodule, mode 160000) entry installs successfully instead of erroring with `failed to find blob ...`.
- [x] #2 Only blobs, executables, and symlinks are materialized; a submodule path produces no file and no directory in the installed output, and executable/symlink extraction still works.
- [x] #3 The `export-ignore` attribute handling in `extract_tree()` continues to work unchanged.
- [x] #4 Regression coverage: a hermetic integration test builds a bare repo containing a gitlink entry (via `git update-index --add --cacheinfo 160000,<sha>,<path>` with a commit OID not present in the parent repo) and asserts sync succeeds and the submodule path is absent from the project; unit tests cover the mode-classification logic for all five gix `Mode` values (FILE, FILE_EXECUTABLE, SYMLINK, COMMIT, DIR).
- [x] #5 CHANGES.md gains an entry under `[Unreleased] / Fixed` describing submodule support.
- [x] #6 `cargo test`, `cargo clippy`, and `cargo fmt --check` all pass.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Extract `is_materializable(mode)` helper from the extract_tree() filter using equality (==) instead of bitflag contains(), since Mode::COMMIT (0o160000) shares type bits with Mode::SYMLINK (0o120000).
2. Replace the triple-negative contains() filter in extract_tree() with `if !is_materializable(entry.mode) { continue; }`.
3. Add `with_gitlink(path, commit_sha)` to tests/common/git_fixtures.rs BareRepoBuilder; writes mode-160000 entries via `git update-index --add --cacheinfo 160000,<sha>,<path>` before the initial commit.
4. Add integration test in tests/git_fixtures.rs building a repo with a real file plus a gitlink, running gg sync, asserting success, file installed, and submodule path absent.
5. Add unit tests in src/dependency/cache.rs for is_materializable() covering all five gix Mode values, plus a regression test asserting Mode::COMMIT.contains(Mode::SYMLINK) is true.
6. Add CHANGES.md entry under [Unreleased]/Fixed.
7. Run cargo test, clippy, fmt.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented is_materializable() helper (equality check on FILE/FILE_EXECUTABLE/SYMLINK) and switched extract_tree() to use it. Added with_gitlink() builder + integration test sync_succeeds_with_gitlink_submodule_entry plus three unit tests. Verified the integration test reproduces the original failure ("failed to find blob aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa") against the old filter before restoring the fix. All 231 unit + 304 total tests pass; clippy and fmt clean.

Validation: integration test sync_succeeds_with_gitlink_submodule_entry passes with the fix and reproduces the original failure ("failed to find blob aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa") against the pre-fix filter. Full suite: 304 tests pass (231 unit, 73 integration), cargo clippy --all-targets and cargo fmt --check clean. export-ignore handling verified unchanged by code review (branch untouched; extract_tree still exercised by existing git sync integration tests).

$Review feedback applied: renamed BareRepoBuilder::with_gitlink/field to with_submodule/submodules and the integration test to sync_succeeds_with_submodule_entry (caller-facing submodule terminology); CHANGES.md entry now appends issue link ([#3](https://github.com/godotneers/ggg/issues/3)). Re-ran full suite (304 tests pass), clippy and fmt clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed git dependencies with submodules failing during extraction. In src/dependency/cache.rs, replaced the bitflag contains() mode filter in extract_tree() with an is_materializable() helper using equality against FILE/FILE_EXECUTABLE/SYMLINK, so gitlink entries (Mode::COMMIT, 0o160000) are skipped instead of reaching write_blob() and erroring on a commit OID that lives only in the submodule repo - matching git archive / GitHub tarball semantics. Added with_gitlink() to the BareRepoBuilder fixture, integration test sync_succeeds_with_gitlink_submodule_entry, and three unit tests covering all five gix Mode values plus a contains() overlap regression test. Added a CHANGES.md [Unreleased]/Fixed entry. Verified: the integration test reproduces the original "failed to find blob ..." error against the pre-fix filter and passes after; full suite 304 tests pass (231 unit + 73 integration); cargo clippy --all-targets and cargo fmt --check clean.
<!-- SECTION:FINAL_SUMMARY:END -->
