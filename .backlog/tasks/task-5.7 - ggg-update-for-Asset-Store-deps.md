---
id: TASK-5.7
title: ggg update for Asset Store deps
status: Done
assignee: []
created_date: '2026-09-14 07:00'
updated_date: '2026-09-22 04:49'
labels: []
dependencies:
  - TASK-5.4
parent_task_id: TASK-5
priority: medium
type: task
ordinal: 27000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend ggg update so it also manages Asset Store deps, which can update releases the way the asset library path updates versions today. For a store dep it compares the locked release against the latest stable Godot-compatible release and, when newer, drops the lock entry so the next ggg sync resolves and downloads it. Keeps the existing asset library behavior and the existing rejection message for git/archive deps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ggg update recognises AssetStore deps in addition to AssetLib; when a newer stable Godot-compatible release exists it drops the lock entry and instructs the user to run ggg sync, mirroring the asset library path
- [x] #2 Already-latest store deps print up to date; dry-run reports the pending update with release id + version without mutating ggg.lock
- [x] #3 Git/archive deps are still rejected with the existing clear message; running with no store or library deps prints the existing no-op message
- [x] #4 CHANGES.md and docs/reference/commands/update.md updated for store support
- [x] #5 Tests extended following tests/update.rs patterns; cargo test, cargo clippy, cargo fmt --check all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Cargo.toml: add versions = "8" (semantic free-form version comparison, pure Rust).
2. src/godot/asset_store.rs: add pub(crate) cmp_store_versions(a,b) -> Ordering (normalize: trim + strip leading v/V; Versioning::new both; parsed outranks unparseable; unparseable fall back to lexical) and pub(crate) select_latest_compatible(releases, godot): filter stable && is_compatible_with, max by (version cmp, then release_id). Unit tests incl. 5.0.0(id120)/3.9.2(id121)/4.0.4 -> picks 5.0.0, v-prefix equivalence, 4-part versions, prerelease, unparseable fallback, duplicate-version > higher id.
3. src/commands/add.rs resolve_store_release: None branch uses select_latest_compatible (replaces .max_by_key(id)) so add and update agree on newest.
4. src/commands/update.rs: support Source::AssetStore alongside AssetLib. Collect both; name-filtered rejects git/archive with broadened message. No candidates -> "No asset library or asset store dependencies in ggg.toml.". Store per-dep: locked = lock.locked_store(name, pub, asset, pinned); none -> existing no-lock sync hint and skip; fetch releases, select_latest_compatible; none -> per-dep skip note; cmp_store_versions(latest, pinned) != Greater -> "up to date (v{pinned})."; Greater -> dry-run prints "version {pinned} -> v{latest} (release id {id})." without writing; real run bumps ggg.toml pinned version (AssetStoreRef.version) + lock.remove, saves ggg.lock + ggg.toml when any_updated. No downgrade: only Greater triggers update.
5. src/main.rs: update Update subcommand help to mention Asset Store.
6. docs/content/docs/reference/commands/update.md + CHANGES.md (Unreleased entry).
7. tests/update.rs: update two message assertions; add offline store no-lock hint; wiremock: up-to-date, newer drops lock + bumps ggg.toml pin, --dry-run reports release id + version and mutates neither file, non-monotonic ordering test. Tests seed lock via ggg sync against mount_store_releases_with_archives.
8. Verify: cargo fmt --check, cargo clippy, cargo test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Validation: 21 godot::asset_store unit tests pass (incl. prerelease<release ordering via pad_two_part, v-prefix/4-part/unparseable fallback); 13 tests/update.rs pass (new: store up-to-date, newer drops lock + bumps ggg.toml pin, --dry-run mutates neither file, 5.0.0/3.9.2/4.0.4 ordering, offline no-lock hint); cargo fmt --check, cargo clippy --all-targets, cargo test all green.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
ggg update now manages Asset Store deps: compares the locked release against the newest stable Godot-compatible release by semantic version (versions 8 crate; release id only breaks equal-version ties; never downgrades), drops the lock entry and bumps the ggg.toml pinned version on a real run, reports release id + version on --dry-run without mutating files. ggg add's bare-spec resolution shares select_latest_compatible so add/update agree. Verified with 21 asset_store unit tests and 13 update integration tests (incl. non-monotonic 4.0.4->5.0.0-not-3.9.2), plus cargo clippy and cargo fmt --check.
<!-- SECTION:FINAL_SUMMARY:END -->
