---
id: TASK-1.3
title: 'Wiremock spike: manifest + asset-library round-trip through the lib'
status: Done
assignee: []
created_date: '2026-09-08 05:27'
updated_date: '2026-09-11 05:11'
labels: []
dependencies:
  - TASK-1.2
parent_task_id: TASK-1
type: task
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
De-risk wiremock-on-Windows and the env override investment early. New tests/common/wiremock.rs: a MockApi that starts the server and mounts the versions-manifest and asset-library stubs, with response bodies injected with the server port at runtime.
Proof tests (library-level, one test file): set GGG_GODOT_MANIFEST_URL and call ggg::godot::manifest::fetch_versions() against wiremock asserting the parsed result; same for asset_lib::search and get_asset.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 MockApi starts a wiremock server and mounts manifest + asset-library stubs internally
- [x] #2 Stub response bodies embed the live server port at runtime
- [x] #3 fetch_versions() round-trips against wiremock and parses correctly
- [x] #4 asset_lib::search and get_asset round-trip via wiremock
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add tests/common/wiremock.rs defining MockApi holding a wiremock MockServer. Provide MockApi::start() (async) that starts the server, plus separate mount methods mount_manifest() and mount_asset_lib(). Expose ase_url() and the env-var name->value wiring helpers.
2. Mount methods use response bodies that embed the live server port at runtime (format! the YAML/JSON with base_url()), so AC #1 and #2 are met. The asset stub's download_url self-references the same server's base_url() to stay hermetic; get_asset stub matches path /asset/{id} and search matches /asset.
3. Register tests/common/wiremock.rs in tests/common/mod.rs via pub mod wiremock;.
4. Create tests/wiremock.rs (the library-level proof test file, mod common;) with #[tokio::test] + #[serial] tests:
   - manifest: set GGG_GODOT_MANIFEST_URL to mock base_url(), call ggg::godot::manifest::fetch_versions() via tokio::task::spawn_blocking, assert the parsed result (AC #3).
   - asset lib: set GGG_ASSET_LIB_API_URL to mock base_url(), call asset_lib::search and asset_lib::get_asset via spawn_blocking, assert parsed results incl. self-referencing download_url (AC #4).
   - Use spawn_blocking (not direct calls) to avoid starvation/deadlock on the async runtime.
5. Run cargo test, cargo clippy --all-targets, cargo fmt --check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented:
- tests/common/wiremock.rs: MockApi wrapping a wiremock MockServer. MockApi::start() (async); base_url() maps to server.uri(); mount_manifest(YAML) and mount_asset_lib(search_json, detail_json), both replacing {base} in the response body with the live server uri() at runtime so reported download URLs self-reference the mock. get_asset stub matches path_regex ^/asset/[0-9]+$; search matches /asset.
- tests/common/mod.rs: added pub mod wiremock.
- tests/wiremock.rs: three #[tokio::test] + #[serial] library-level proof tests (fetch_versions, asset_lib::search, asset_lib::get_asset). Each sets the relevant GGG_*_URL env var to api.base_url() and invokes the blocking library call through tokio::task::spawn_blocking to avoid starving the async runtime that serves wiremock. #[serial] guards the process-global env mutation. get_asset test asserts download_url equals base_url()/files/starter-template.zip (live port injected) and that empty download_hash parses to None.

Note: wiremock 0.6.5 (the current release, pinned on the 0.6 line) exposes MockServer::uri() for the live server address; MockApi::base_url() is our own convenience that maps to it.

Validation: cargo test run on the wiremock target (3 passed); full cargo test (233 passed: 216 unit, 6 cli, 3 deps, 5 remove, 3 wiremock); cargo clippy on all targets clean; cargo fmt --check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
- Created tests/common/wiremock.rs with a MockApi helper:
  - MockApi::start() (async) -> wiremock MockServer; base_url() maps to server.uri()
  - Separate mount_manifest() and mount_asset_lib() stubs whose response bodies inject the live server port at runtime, so reported asset download_urls self-reference the mock
- Registered it in tests/common/mod.rs.
- Added tests/wiremock.rs with three library-level proof tests (fetch_versions, asset_lib::search, asset_lib::get_asset) that set GGG_GODOT_MANIFEST_URL / GGG_ASSET_LIB_API_URL to the mock and call the blocking lib functions through tokio::task::spawn_blocking; #[serial] guards process-global env mutation.
- Verified: cargo test (233 passed incl. 3 new wiremock tests), cargo clippy --all-targets clean, cargo fmt --check clean.
<!-- SECTION:FINAL_SUMMARY:END -->
