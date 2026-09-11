---
id: TASK-1.5
title: 'sync test suite (seeded Godot cache, git + archive deps)'
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
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
One new tests/sync.rs plus a TestProject Godot-cache seeding helper. Installs deps and writes ggg.lock / .ggg.state / .gitignore; dry-run writes nothing; conflicts block without --force; --force overwrites; stale files cleaned up after removing a dep.

Also covers the offline error/edge paths: fail with 'no ggg.toml found'; empty dependency list + dry-run reports nothing to install; --force overwrites files not under ggg's control (distinct from user-modified owned files).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Fails with 'no ggg.toml found' when no config exists
- [x] #2 Empty dependency list + dry-run reports nothing to install
- [x] #3 Install writes ggg.lock, .ggg.state and updates .gitignore
- [x] #4 --dry-run prints the plan without writing files
- [x] #5 Modified owned file blocks without --force
- [x] #6 --force overwrites user-modified owned files
- [x] #7 --force overwrites files not under ggg's control
- [x] #8 Removes stale files after a dep is removed/remapped
- [x] #9 TestProject includes a Godot-cache seeding helper
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Harness extensions (tests/common/mod.rs): expose pub seed_godot(release); add write_config(config) that saves ggg.toml and auto-seeds Godot cache.
2. Wiremock extension (tests/common/wiremock.rs): add mount_file(path, bytes) serving raw bytes.
3. Add tests/common/archive.rs helper zip_bytes(entries) via zip dev-dep.
4. Write tests/sync.rs covering the 9 ACs + [sync] force_overwrite globs + full archive behavior parity (happy path, modified-owned block, --force overwrite, stale-removal via URL remap).
5. Add note to TASK-1.9 to extend sync tests with asset-lib e2e.
6. Run cargo test, cargo clippy, cargo fmt --check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Validation: cargo test --test sync passes 13/13 (all 9 ACs covered, plus [sync] force_overwrite globs and full archive parity). cargo test full suite passes (216 unit + integration). cargo clippy --all-targets clean. cargo fmt --check clean.

Ground-truth (final):
- ConfigBuilder owns config authoring (config().git/archive/asset/sync_force_overwrite().write()); dep methods replace by name; write() replaces the full config. ConfigBuilder::new() pre-populates godot/sync/deps from an existing ggg.toml (no conditional-load/godot_changed flag).
- write_config() and seed_godot() are private helpers inside ConfigBuilder; seed_godot derives the dummy exe name from the release string (Godot_<release>.exe / godot_<release>_linux), so any pinned Godot version seeds correctly.
- TestProject::write(rel, contents) simulates user edits.
- MockApi gained mount_file(route, bytes) for archive bytes; new tests/common/archive.rs zip_bytes helper; tests/sync.rs added with 13 tests.

AC mapping:
- #1: sync_fails_without_ggg_toml
- #2: sync_dry_run_with_no_deps_reports_nothing
- #3: sync_installs_and_writes_lock_state_gitignore
- #4: sync_dry_run_prints_plan_but_writes_nothing
- #5: sync_modified_owned_file_blocks_without_force
- #6: sync_force_overwrites_modified_owned_file
- #7: sync_force_overwrites_unmanaged_file (+ sync_force_overwrite_glob_overrides_without_force_flag for the '[sync] force_overwrite' globs, which the builder targets via sync_force_overwrite(&['**/*.import']))
- #8: sync_removes_stale_files_after_dep_removed + sync_removes_stale_files_after_dep_remapped
- #9: covered by the harness Godot-cache seeding helper (ConfigBuilder::seed_godot), used by every test

TASK-1.9 noted to extend these tests with asset-lib deps once its scaffolding exists.

Post-review design changes (after approval):
- ConfigBuilder is now the single config-authoring path. ConfigBuilder::new() pre-populates godot/sync/deps from an existing ggg.toml if present (removes the earlier conditional-load + godot_changed flag in write()); dep methods (git/archive/asset) replace by name; write() replaces the full config.
- Added ConfigBuilder::sync_force_overwrite(patterns) so AC#7b no longer hand-constructs a Config - it uses config().sync_force_overwrite(&['**/*.import']).git(...).write().
- write_config() and seed_godot() moved INTO ConfigBuilder as private helpers (they were only used there). seed_godot now derives the dummy exe name from the release string instead of the hardcoded 4.3-stable name, so non-default Godot versions still seed correctly.
- tests/sync.rs: every test now has an inline comment block explaining the business rule it verifies (ownership semantics, conflict blocking, --force override, dry-run side-effect-freedom, stale cleanup, force_overwrite glob escape hatch, and archive <-> git parity).

Re-verified: cargo test (216 unit + all integration incl. 13 sync) passes; cargo clippy --all-targets clean; cargo fmt --check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented the sync end-to-end test suite (tests/sync.rs, 13 tests) covering all 9 acceptance criteria, plus [sync] force_overwrite globs and full archive-dependency parity (happy-path install, modified-owned block + --force overwrite, stale-removal via URL remap).

Harness:
- ConfigBuilder is the single config-authoring path. ConfigBuilder::new() pre-populates godot/sync/deps from an existing ggg.toml; dep methods (git/archive/asset) replace by name (remap = same name, new source); added sync_force_overwrite(patterns) so no test hand-builds a Config; write() replaces the full config and auto-seeds the Godot cache via the private seed_godot() helper. seed_godot derives the dummy exe name from the release string (works for any Godot version).
- TestProject::write(rel, contents) simulates user file edits; write_config/seed_godot are private to ConfigBuilder (not part of the public TestProject API).
- MockApi: added mount_file(route, bytes) to serve raw archive/download bytes over wiremock.
- New tests/common/archive.rs zip_bytes helper for building in-memory zips.

AC evidence (each maps to a passing test):
- #1 sync_fails_without_ggg_toml
- #2 sync_dry_run_with_no_deps_reports_nothing
- #3 sync_installs_and_writes_lock_state_gitignore
- #4 sync_dry_run_prints_plan_but_writes_nothing
- #5 sync_modified_owned_file_blocks_without_force
- #6 sync_force_overwrites_modified_owned_file
- #7 sync_force_overwrites_unmanaged_file + sync_force_overwrite_glob_overrides_without_force_flag
- #8 sync_removes_stale_files_after_dep_removed + sync_removes_stale_files_after_dep_remapped
- #9 via the harness Godot-cache seeding helper (now ConfigBuilder::seed_godot)

Every test carries an inline comment block explaining the business rule it verifies. Verified: cargo test --test sync (13 passed), full cargo test (216 unit + integration all pass), cargo clippy --all-targets (clean), cargo fmt --check (clean).

TASK-1.9 was noted to extend these sync tests with asset-lib deps once its scaffolding exists.
<!-- SECTION:FINAL_SUMMARY:END -->
