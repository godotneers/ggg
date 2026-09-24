---
id: TASK-5.10
title: >-
  Make ggg update fail fast on lock/config drift and reconcile ggg.lock in ggg
  sync
status: Done
assignee: []
created_date: '2026-09-22 06:10'
updated_date: '2026-09-22 06:13'
labels: []
dependencies: []
parent_task_id: TASK-5
ordinal: 30000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Review of TASK-5.7 (`ggg update` for Asset Store deps) flagged that the command updated dependencies one at a time: a dep with no lock entry was skipped, a stale lock entry or a lock-key edit in ggg.toml was never noticed, and git<->archive source-kind swaps could leave the command comparing against the wrong version. Worse, an update that touched several deps could partially apply before one of these cases surfaced. Separately, ggg sync never pruned ggg.lock entries for dependencies removed from ggg.toml, so stale entries persisted forever (breaking update/diff later). This task hardens both commands: update validates the whole selection up front and aborts on any disagreement, and sync prunes/rewrites lock entries to keep ggg.lock derived from ggg.toml.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ggg sync prunes ggg.lock entries for dependencies no longer declared in ggg.toml
- [x] #2 ggg sync rewrites an entry in place when a dependency changes source kind (e.g. git to archive), leaving no fields of the old kind
- [x] #3 ggg update without a name aborts with a listing of every config/lock disagreement (missing lock entry for an asset-lib/store dep, stale lock entry, kind mismatch, lock-key edit) before any API call or file write
- [x] #4 ggg update <name> validates only the named dependency, still refuses git/archive dependencies and unknown names, and does no network or write on failure
- [x] #5 Store updates are atomic: the pinned version bump and lock-entry drop are persisted only after every target passes preflight; failed runs leave ggg.toml and ggg.lock byte-identical
- [x] #6 CHANGES.md documents the fixes and the update command docs describe the lock-consistency preflight
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add SourceKind to config.rs (Git/Archive/AssetLib/AssetStore) with Display, plus Dependency::kind().
2. Add drift machinery to LockFile: LockEntry::kind(), LockFile::drift(&Config), lock.check_dependency(dep), Drift struct, identity mismatch helpers.
3. Run a lockfile drift preflight in commands/update.rs before any API call or write; bail with a listing on any drift; git/archive dep without a lock entry is NOT drift. Named mode uses Config::get_dependency + check_dependency.
4. Build UpdateTarget from lock-validated data (locked asset_version for asset-lib, locked release_version/identity for store); store pin bump + lock.remove only after the whole comparison loop; single lock.save + config.save at the end (atomic).
5. commands/sync.rs: prune lock entries not declared in ggg.toml right before lock.save (kind changes already rewritten by upsert-by-name).
6. Tests: drift unit tests in lockfile.rs; flip two unlocked-dep update tests to failures; add stale-lock, kind-mismatch, store-pin-edit, atomicity tests in tests/update.rs; add prune-on-remove + git->archive kind rewrite tests in tests/sync.rs.
7. Docs: update.md lock-consistency section; CHANGES.md Fixed bullets for both fixes.
8. cargo fmt --check, clippy --all-targets, cargo test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation complete. Summary of changes:
- src/config.rs: SourceKind enum + Dependency::kind().
- src/dependency/lockfile.rs: LockEntry::kind(), LockFile::drift(), check_dependency(), Drift struct, drifts_for_dep/identity helpers; pub(crate) entry(); 12 new unit tests (kind classification, 9 drift scenarios).
- src/commands/update.rs: rewritten around a drift preflight. All-mode bails listing every disagreement; named mode validates via get_dependency + check_dependency. UpdateTarget bases come from the lock entry. Writes only after the whole loop. Git/archive deps rejected; missing lock entry for asset-lib/store is a hard error.
- src/commands/sync.rs: prunes lock entries not in ggg.toml before save.
- tests/update.rs: 3 tests flipped/renamed to failures (unlocked lib, unlocked store, + existing reject/named coverage intact); added stale-lock, kind-mismatch, store-pin-edit, atomicity tests (17 total, all pass).
- tests/sync.rs: added prune-on-remove (via ggg remove + sync) and git->archive kind-rewrite tests (26 total, all pass).
- docs/content/docs/reference/commands/update.md: Lock-consistency preflight section added.
- CHANGES.md: Fixed subsection with two bullets.
- Verification: cargo fmt --check clean, cargo clippy --all-targets clean, cargo test green (292 lib + all integration suites).

Validation (2026-09-22): cargo test passes 405 tests total incl. 12 new lockfile drift unit tests, 18 update integration tests (incl. named-drift offline test), 26 sync integration tests (prune-on-remove via `ggg remove beta` + git->archive kind rewrite). cargo fmt --check and cargo clippy --all-targets clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Harden `ggg update` into a fail-fast, lock-driven command and make `ggg sync` reconcile ggg.lock as a derivative of ggg.toml (review follow-up to TASK-5.7). Added SourceKind + Dependency::kind(); LockFile::drift(), check_dependency(), LockEntry::kind(), and Drift message generation; update now runs a preflight that bails on any config/lock disagreement (missing lock entry for asset-lib/store deps, stale entries, git<->archive or store<->lib kind swaps, lock-key edits incl. store pin and asset-lib id) before any network call or write, uses lock entries as comparison bases, and persists the store pin bump + lock drop atomically at the end. sync now prunes lock entries absent from ggg.toml (kind changes are rewritten in place by the existing upsert-by-name). Fixed entries added to CHANGES.md; update.md documents the lock-consistency preflight. Verified: cargo fmt --check clean, cargo clippy --all-targets clean, cargo test all green (405 tests incl. new drift/prune/atomicity/named-mode coverage).
<!-- SECTION:FINAL_SUMMARY:END -->
