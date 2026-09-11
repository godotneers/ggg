---
id: TASK-1.6
title: diff test suite
status: Done
assignee: []
created_date: '2026-09-08 05:27'
updated_date: '2026-09-11 05:11'
labels: []
dependencies:
  - TASK-1.3
  - TASK-1.4
parent_task_id: TASK-1
type: task
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
tests/diff.rs with seeded cache + installed state: clean project exits 0 with 'no modified files'; a modified owned file exits 1 and prints a unified diff; diff <file> filters to a single file; binary files are skipped with a note. Also fail with 'no ggg.toml found'.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Fails with 'no ggg.toml found' when no config exists
- [x] #2 Clean project exits 0 with 'no modified files'
- [x] #3 Modified owned file exits 1 and prints a unified diff
- [x] #4 diff <file> filters to a single file
- [x] #5 Binary file is skipped with a note
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Phase 1: Shared FileContent fixture type (tests/common/mod.rs)
- FileContent enum (Text/String, Bytes/Vec<u8>) with as_bytes() and From impls for &str, String, &[u8], Vec<u8>
- TestProject::write(rel, impl Into<FileContent>) accepts text or raw bytes (no write_bytes helper)

Phase 2: BareRepo builder (tests/common/git_fixtures.rs)
- BareRepo::builder() -> BareRepoBuilder.with(path, impl Into<FileContent>).build()
- build() runs git init/add/commit/clone; uses contents.as_bytes()
- Update existing callers in sync.rs and git_fixtures.rs (11 sites, all text, unchanged textually)

Phase 3: Diff tests in tests/diff.rs
- AC#1 diff_fails_without_ggg_toml: empty project, failure + stderr contains 'no ggg.toml found'
- AC#2 diff_clean_project_exits_zero: sync a dep, success + stderr contains 'no modified files'
- AC#3 diff_modified_file_shows_unified_diff: modify installed file, exit 1 + @@ hunk in stdout
- AC#4 diff_file_filter: 2 files, modify both, diff <one>, only that file in output
- AC#5 diff_binary_file_skipped: binary repo file via with(), overwrite via write(), 'binary file, skipping'

Phase 4: Verify - cargo test, cargo clippy --all-targets, cargo fmt --check
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Refactored BareRepo::new(&[(&str,&str)]) into a builder (BareRepo::builder() -> .with(path,&str) / .with_bytes(path,&[u8]) -> .build()) so repos can contain binary files; updated all 11 call sites in sync.rs and git_fixtures.rs. Added TestProject::write_bytes(rel, &[u8]). Verified binary files survive the git->cache->project pipeline (blob bytes via write_blob, fs::copy). Validation: cargo test --test diff (5/5), full cargo test (216 unit + all integration), cargo clippy --all-targets, cargo fmt --check all pass.

Post-review revision: FileContent (Text/Bytes enum with as_bytes() and From impls for &str/String/&[u8]/Vec<u8>) moved into tests/common/mod.rs, shared by TestProject::write (now takes impl Into<FileContent>, write_bytes removed) and the BareRepo builder (with_bytes merged into with(), build() uses as_bytes()). Text call sites unchanged; only the diff binary test updated. Kept builder over new() since a homogeneous slice would force .into() on every text file. Verified: full cargo test, clippy, fmt all clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Created tests/diff.rs with 5 integration tests covering all acceptance criteria (no ggg.toml fails; clean exits 0; modified owned file exits 1 with a unified diff; file filter; binary skip). Added a shared FileContent enum in tests/common/mod.rs (Text/Bytes + From conversions for &str/String/&[u8]/Vec<u8> + as_bytes()) used by TestProject::write(rel, impl Into<FileContent>) and the new BareRepo builder (builder().with(path, impl Into<FileContent>).build()), replacing the original slice constructor. Verified by cargo test --test diff (5 passed), full cargo test (all pass), cargo clippy --all-targets (clean), and cargo fmt --check (clean).
<!-- SECTION:FINAL_SUMMARY:END -->
