---
id: TASK-1.9
title: search test suite (wiremock asset lib)
status: Done
assignee: []
created_date: '2026-09-08 05:27'
updated_date: '2026-09-11 05:11'
labels: []
dependencies:
  - TASK-1.3
parent_task_id: TASK-1
type: task
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
tests/search.rs via wiremock asset lib: no-results message; table header + result rows; reads the version filter from ggg.toml; --godot-version overrides ggg.toml; 'Showing X of Y' truncation message; no-ggg.toml versionless path. Plus unit tests for the truncate and digits helpers in src/commands/search.rs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 No results prints 'No results for ...'
- [x] #2 Prints table header + result rows
- [x] #3 Uses ggg.toml version as the filter
- [x] #4 --godot-version overrides ggg.toml
- [x] #5 Works without ggg.toml (no version filter)
- [x] #6 'Showing X of Y' truncation message
- [x] #7 Unit tests cover the truncate and digits helpers
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add received_requests() accessor to tests/common/wiremock.rs for querystring assertions.
2. Create tests/search.rs with 6 wiremock integration tests (no-results msg, table header+rows, ggg.toml version filter, --godot-version override, no-ggg.toml absent param, Showing X of Y).
3. Fix truncate() in src/commands/search.rs for UTF-8-safe truncation and add unit tests for truncate + digits.
4. Extend tests/sync.rs with asset-lib parity tests (install, modified-owned block + force, remap stale removal).
5. Verify: cargo test, cargo clippy, cargo fmt --check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
From TASK-1.5: tests/sync.rs was written for git + archive deps only. When this task's asset-library scaffolding exists, extend tests/sync.rs (or an asset-lib-specific suite) to cover asset-lib deps end-to-end through ggg sync: install (reuse MockApi::mount_asset_lib + MockApi::mount_file for the download ZIP), modified-owned block + --force overwrite, and stale-removal via remapping. See the archive parity tests in tests/sync.rs as the template.

Validation (all passed): cargo test (220 unit + 6 search + 16 sync incl. 3 new asset-lib tests), cargo clippy --all-targets --all-features, cargo fmt --check.
Decisions/notes:
- Version-filter assertions (AC #3/#4/#5) use a new MockApi::received_requests() accessor so tests assert the actual godot_version query param ggg sent, not just the rendered output label.
- Added MockApi::mount_asset_detail_for(id, body) for scenarios needing per-id detail responses (asset-lib remap test); mount_asset_detail stays for single-asset cases.
- truncate() in src/commands/search.rs was fixed to cut at a UTF-8 char boundary (byte slicing previously panicked on multi-byte titles/authors) and is covered by unit tests.
- Asset-lib sync tests use zips with a wrapper dir because AssetLib defaults to strip_components = 1; like the archive helpers, each asset gets its own download route so several assets coexist on one mock server.

Code-review follow-up (2026-09-10): addressed three review comments.
- Unified detail stubbing: dropped mount_asset_detail (no-id) entirely; mount_asset_detail_for is now the only detail endpoint helper.
- mount_asset_detail_for now takes the download archive as Option<Vec<u8>> and, when present, owns the download URL entirely: it mounts the zip at /files/asset-{id}.zip, rewrites the body download_url to that route, and skips the separate mount_file call in tests. Callers pass a throwaway download_url (String::new()) when the mock owns it.
- Removed the redundant summary() helper in tests/search.rs; AssetSummary::new handles literals directly.
Migrated all callers (tests/wiremock.rs, update.rs, ls_dep.rs, sync.rs mount_asset helper) and doc references in tests/common/mod.rs.
Re-verified: cargo test, cargo clippy --all-targets --all-features, cargo fmt --check all pass.

Code-review iteration 2 (mount_asset_detail_for -> mount_asset_detail): per reviewer feedback on tests/ls_dep.rs:215:
- The archive now lives IN AssetDetailBody as a #[serde(skip)] file: Option<Vec<u8>> field, set via AssetDetailBody::with_file(bytes). URL ownership is a property of the body, so a conflicting download_url can no longer be configured separately.
- mount_asset_detail_for is renamed to mount_asset_detail(id, body); it branches on the body: file attached -> mock owns the URL (serves /files/asset-{id}.zip, rewrites download_url, mounts the file); no file -> body download_url (with {base} swap) is reported verbatim. No Option<Vec<u8>> parameter remains to be misused.
- Two shapes, one function: update.rs/ls_dep.rs/sync.rs use the bytes shape via .with_file(); wiremock.rs round-trip uses the url shape.
Re-verified: cargo test, cargo clippy --all-targets --all-features, cargo fmt --check all pass.

Code-review iteration 3 (constructor overloads): per reviewer feedback on tests/common/wiremock.rs:157.
- AssetDetailBody now has two mutually exclusive constructors: new(...) takes a download_url (no archive) and new_with_file(...) takes the archive bytes and OMITS download_url entirely (the mock rewrites it at mount time). No credentials/url args to give in the file case; the old with_file builder is removed.
- update.rs detail() helper now takes zip: Option<Vec<u8>> and dispatches to the right constructor; sync.rs mount_asset and ls_dep.rs use new_with_file directly.
Re-verified: cargo test, cargo clippy --all-targets --all-features, cargo fmt --check all pass.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
- Created tests/search.rs with 6 wiremock integration tests: no-results msg, table header+rows, ggg.toml version filter, --godot-version override, no-ggg.toml sends no filter param, Showing X of Y.
- Added search.rs unit tests for truncate and digits, and fixed truncate to cut at UTF-8 char boundaries.
- Extended tests/sync.rs with 3 asset-lib parity tests: install, modified-owned block + force, id-remap stale removal.
- Added MockApi::received_requests and mount_asset_detail_for helpers.
- Verified: cargo test, cargo clippy --all-targets --all-features, cargo fmt --check all pass.
<!-- SECTION:FINAL_SUMMARY:END -->
