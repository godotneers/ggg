---
id: TASK-5.11
title: Unify lock-entry reuse in the resolver behind a verified_entry gate
status: Done
assignee: []
created_date: '2026-09-24 05:22'
updated_date: '2026-09-24 05:54'
labels: []
dependencies: []
parent_task_id: TASK-5
priority: medium
type: enhancement
ordinal: 31000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The resolver looks up a usable lockfile entry via four per-kind getters (get_git_entry, get_archive_entry, get_asset_lib_entry, get_store_entry) that each partially re-implement the identity matching now centralized in determine_drift_status/identity_matches. The drift machinery (check_dependency) is a strict superset of those getters' checks: no drift means the lock entry is a faithful snapshot of ggg.toml. This refactor makes 'is this lock entry usable?' a single question answered by one pub(crate) helper, so resolver/ensure agree on when the lock was reused and the duplicated getter logic can be deleted. It also fixes inconsistent handling of lock entries that pass identity checks but are missing resolved-value fields: git/archive currently fall through to fresh resolution while assetlib/store hard-error with 'missing url/archive_sha' - both should resolve fresh.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Resolver reuses a lock entry only when it is faithful to ggg.toml (no drift) AND carries every required resolved-value field; any other entry is resolved fresh instead of hard-erroring
- [x] #2 Lock entries missing a verified resolved value (git sha, archive archive_sha, assetlib/store url or archive_sha, assetlib asset_version) are treated as unusable and re-resolved/re-locked
- [x] #3 get_git_entry/get_archive_entry/get_asset_lib_entry/get_store_entry are removed; identity_matches plus the new verified_entry helper are the single source of truth for lock lookups
- [x] #4 ensure.rs's locked-commit-missing recovery fallback uses the same predicate as the resolver's reuse decision
- [x] #5 Reuse keeps producing the existing 'locked ...' notes; cargo test, cargo clippy, and cargo fmt --check are green
- [x] #6 Integration tests in tests/sync.rs verify incomplete lock entries are re-resolved and re-locked successfully (assetlib/store missing url/archive_sha, assetlib missing asset_version), seeded via typed LockBuilder fixtures
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. tests/common/mod.rs: add typed LockBuilder methods asset_lib_unresolved, store_unresolved, asset_lib_without_version (serialize through production LockFile::save, no raw TOML). 2. tests/sync.rs: add sync_relocks_incomplete_asset_lib_lock, sync_relocks_incomplete_store_lock, sync_relocks_asset_lib_lock_without_version against wiremock MockApi, asserting success + re-locked complete entries. 3. lockfile.rs: drop verified_entry_returns_none_when_asset_lib_has_no_version and verified_entry_returns_none_when_store_entry_missing_url_or_sha (shadowed by the new integration tests). 4. Verify: cargo test, cargo clippy --all-targets, cargo fmt --check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the verified_entry gate. lockfile.rs: added verified_entry(dep) (drift filter via determine_drift_status + new is_complete() completeness check) and deleted get_git_entry/get_archive_entry/get_asset_lib_entry/get_store_entry; store round-trip test rewritten to index entries directly. resolver.rs: resolve_dependency now early-returns from_locked_entry(dep, entry) for any verified entry; the four fresh-resolve arms lost their lock-reuse blocks and the assetlib/store 'missing url/archive_sha' hard-errors (context bails) were removed in favor of falling through to fresh resolution. ensure.rs: git_used_lock now uses lock.verified_entry(dep).is_some().

Validation: cargo test all green (lib 304 passed; integration suites incl. update 19, sync 26, wiremock 6 pass); cargo clippy --all-targets clean; cargo fmt --check clean. New tests: verified_entry faithful/None cases in lockfile.rs; reuse_* resolver tests cover git/archive/assetlib/store locked reuse and stale-entry rejection. Note: cargo fmt also normalized one pre-existing overlong line in src/commands/update.rs (fmt hygiene, no semantic change).

Folded in integration coverage (user request): added typed LockBuilder fixtures in tests/common/mod.rs (asset_lib_unresolved, store_unresolved, asset_lib_without_version) that build incomplete LockEntry values and serialize via production LockFile::save - no hand-written TOML. Added 3 sync tests (sync_relocks_incomplete_asset_lib_lock, sync_relocks_incomplete_store_lock, sync_relocks_asset_lib_lock_without_version) proving incomplete lock entries are re-resolved and re-locked end-to-end against wiremock. Dropped 2 unit tests now shadowed by those integration tests: verified_entry_returns_none_when_asset_lib_has_no_version, verified_entry_returns_none_when_store_entry_missing_url_or_sha. Validation: cargo test all green (lib 302 = 304-2 dropped; sync suite 29 = 26+3); cargo clippy --all-targets clean; cargo fmt --check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Folded integration coverage for incomplete lock entries into TASK-5.11 per user request. Added typed LockBuilder fixtures (tests/common/mod.rs): asset_lib_unresolved, store_unresolved, asset_lib_without_version build structurally-incomplete LockEntry values serialized through the production LockFile::save (no hand-written TOML). Added 3 end-to-end sync tests (tests/sync.rs) proving ggg sync re-resolves and re-locks incomplete assetlib/store entries against a wiremock API instead of hard-erroring, and records asset_version that was previously silently dropped. Dropped the 2 lockfile unit tests shadowed by these integration tests (asset_lib_has_no_version, store_missing_url_or_sha). Verified objectively: cargo test green (lib 302, sync 29 incl. the 3 new tests), cargo clippy --all-targets clean, cargo fmt --check clean. All 6 ACs checked.
<!-- SECTION:FINAL_SUMMARY:END -->
