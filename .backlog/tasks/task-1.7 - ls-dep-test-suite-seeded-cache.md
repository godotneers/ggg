---
id: TASK-1.7
title: ls-dep test suite (seeded cache)
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
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
tests/ls_dep.rs with seeded cache: collapsed tree view; --all flat listing of every file path; git header shows rev -> sha[:8]; archive header shows sha[:8]; unknown-name error; fail with 'no ggg.toml found'.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Fails with 'no ggg.toml found' when no config exists
- [x] #2 Unknown dependency name errors
- [x] #3 Collapsed tree view for a cached dep
- [x] #4 --all lists every file path
- [x] #5 Git header shows rev -> sha[:8]
- [x] #6 Archive header shows sha[:8]
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add tests/ls_dep.rs (sync-first seeded caches: git file:// via BareRepo, archives via wiremock zip, asset via mount_asset_lib if easy).
2. Error tests: no ggg.toml -> failure; unknown name -> failure with dependency-not-found message.
3. Git fixture addons/my-addon/plugin.gd + plugin.cfg: collapsed tree (addons/ and my-addon/ 2 files, header main -> sha8) and --all listing of both paths.
4. Archive fixture with same nesting: tree, --all, and header sha8 (computed via sha2 digest of zip bytes).
5. Asset dep header (asset #1586 v3 -> sha8); drop if heavier than expected.
6. Verify: cargo test --test ls_dep, cargo test, cargo clippy, cargo fmt --check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added tests/ls_dep.rs with 7 tests. Seeding is sync-first: git via BareRepo file:// repos, archive + asset via wiremock zip fixtures; the ls-dep call itself resolves from ggg.lock + cache with no download (ensure_dependency short-circuits on cache.contains). Asset header test included (mount_asset_lib + mount_file for the download URL). Validation: cargo test (full suite 253 tests: 216 unit + 37 integration incl. 7 new) passes; cargo clippy --all-targets clean; cargo fmt --check clean.

Review follow-up: split MockApi::mount_asset_lib into mount_asset_search + mount_asset_detail (tests now mount only what they need; ls_dep asset test no longer needs a dummy search body, wiremock.rs search/detail tests mount only their own endpoint). Added TestProject::env_api(&MockApi) that wires GGG_GODOT_MANIFEST_URL + GGG_ASSET_LIB_API_URL from one mock server, replacing the per-test project.env(..) call in the asset test and simplifying future asset/search/add suites. Verified: full cargo test (254 tests pass), cargo clippy --all-targets clean, cargo fmt --check clean.

Review follow-up (2): inlined single-use fixtures. tests/wiremock.rs no longer declares MANIFEST_YAML/SEARCH_RESULTS/ASSET_DETAIL consts; each response is now built at its mount site. tests/ls_dep.rs inlines its ASSET_DETAIL detail body. tests/common/wiremock.rs: mount_asset_search/mount_asset_detail now take typed serde body structs (AssetSearchBody, AssetSummary, AssetDetailBody) instead of raw JSON strings; numeric fields (asset_id, version) serialize as API-style JSON strings, download_hash is Option serialized to '' when None, so tests get compile-time required-field coverage and only supply what they need. {base} placeholder still replaced with the live server port at mount time. Verified: full cargo test (254 pass), cargo clippy --all-targets clean, cargo fmt --check clean.

Review follow-up (3): added constructor fns to the wiremock body structs (AssetSummary::new, AssetSearchBody::new, AssetDetailBody::new taking impl Into<String>, plus AssetDetailBody::with_download_hash) so tests fill bodies with &str literals instead of .into() per field. AssetDetailBody::new keeps all 8 required fields (compile-time safety) with #[allow(clippy::too_many_arguments)]; download_hash stays optional via the builder setter. Verified: full cargo test (254 pass), cargo clippy --all-targets clean, cargo fmt --check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented the TASK-1.7 ls-dep test suite in tests/ls_dep.rs (7 tests). Covers: no-ggg.toml error, unknown-name error, collapsed tree + rev->sha8 git header, --all flat listing (git + archive), archive sha8 header, and asset 'asset #ID vN -> sha8' header. Verified objectively: cargo test --test ls_dep (7 passed), full cargo test (all passed), cargo clippy --all-targets (no warnings), cargo fmt --check (clean).
<!-- SECTION:FINAL_SUMMARY:END -->
