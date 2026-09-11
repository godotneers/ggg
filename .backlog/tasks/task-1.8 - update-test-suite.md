---
id: TASK-1.8
title: update test suite
status: Done
assignee: []
created_date: '2026-09-08 05:27'
updated_date: '2026-09-11 05:11'
labels: []
dependencies:
  - TASK-1.3
modified_files:
  - tests/update.rs
parent_task_id: TASK-1
type: task
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
tests/update.rs. All offline paths: no ggg.toml, unknown name, non-asset-lib dep, no asset-lib deps in ggg.toml, no lock entry (run ggg sync message). Wiremock-backed paths: up-to-date reports 'up to date'; newer version drops the lock entry and saves ggg.lock; --dry-run reports without modifying ggg.lock. Add any wiremock stubs needed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Fails with 'no ggg.toml found'
- [x] #2 Unknown dependency name errors
- [x] #3 Non-asset-lib dependency rejected with a helpful message
- [x] #4 'No asset library dependencies in ggg.toml.' when none present
- [x] #5 'No lock entry - run ggg sync' message for an unlocked asset dep
- [x] #6 Up-to-date dep reports 'up to date'
- [x] #7 Newer version drops the lock entry and saves ggg.lock
- [x] #8 --dry-run reports without modifying ggg.lock
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Create tests/update.rs with mod common import
2. Implement 5 offline tests using assert_cmd (no wiremock, no ggg sync):
   - no_ggg_toml_fails: no ggg.toml in project dir, assert error message
   - unknown_dep_name_errors: ggg.toml with asset dep, update with wrong name, assert error
   - non_asset_lib_rejected: ggg.toml with git dep, update that dep name, assert helpful message
   - no_asset_lib_deps: ggg.toml with only git deps, update all, assert message
   - no_lock_entry: ggg.toml with asset dep (no sync), assert 'no lock entry' message
3. Implement 3 wiremock-backed tests (#[tokio::test], per-project env only, no #[serial] needed) using ggg sync as setup:
   - up_to_date: MockApi returns version 3, ggg sync populates lock, ggg update reports 'up to date'
   - newer_version: MockApi v3 -> ggg sync -> api.reset() + remount detail as v4 -> ggg update drops lock entry
   - dry_run: Same as newer_version but with --dry-run, assert lock file NOT modified
4. Run cargo test, cargo clippy, cargo fmt --check
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
wiremock 0.6 resolves matching stubs by insertion order (first mount wins), so to serve a newer asset version after ggg sync the old stub must be cleared first. Added MockApi::reset() (wraps MockServer::reset) to tests/common/wiremock.rs. Wiremock tests set env vars only on the spawned command (via TestProject::env), so #[serial] is not required.

cargo test: 216 passed (8 new in tests/update.rs). cargo clippy --all-targets: clean. cargo fmt --check: clean after cargo fmt.

Follow-up: added narrative // comments inside each test explaining what it verifies and why, matching the style of tests/sync.rs. Re-verified: tests/update.rs 8 passed, cargo fmt --check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
- Added tests/update.rs with 8 tests covering every acceptance criterion:
  - 5 offline paths: no ggg.toml, unknown name, non-asset-lib rejection, no asset-lib deps, no lock entry
  - 3 wiremock-backed paths: up to date, newer version drops lock entry, --dry-run leaves ggg.lock untouched
- Wiremock-backed tests seed the lock file realistically via ggg sync, then serve a newer version by re-stubbing the asset detail endpoint.
- Added MockApi::reset() to tests/common/wiremock.rs because wiremock resolves matching stubs by insertion order (first mount wins).
- Verified: cargo test 216 passed, cargo clippy --all-targets clean, cargo fmt --check clean.
<!-- SECTION:FINAL_SUMMARY:END -->
