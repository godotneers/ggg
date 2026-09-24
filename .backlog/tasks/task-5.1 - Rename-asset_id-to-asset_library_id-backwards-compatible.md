---
id: TASK-5.1
title: Rename asset_id to asset_library_id (backwards compatible)
status: Done
assignee: []
created_date: '2026-09-14 06:59'
updated_date: '2026-09-17 04:49'
labels: []
dependencies: []
parent_task_id: TASK-5
priority: medium
type: task
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The config field asset_id is ambiguous once the Asset Store lands. Rename it to asset_library_id while keeping backwards compatibility: old ggg.toml files using asset_id must still load. Pure mechanical rename plus docs; no behavior change. This is the first subtask so the rename is isolated from store work and easy to review.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Dependency.asset_id is renamed to asset_library_id with #[serde(alias = "asset_id")] so existing ggg.toml files load unchanged
- [x] #2 All call sites updated (kind(), validate_source(), new_asset_lib, resolver/lockfile/update/ls_dep/config, tests), audited with ast-grep for asset_id
- [x] #3 Serializer emits asset_library_id for newly added/edited deps; existing deps round-trip unchanged
- [x] #4 docs/reference/configuration.md asset_id section renamed; CHANGES.md entry notes the rename is backwards compatible
- [x] #5 cargo test, cargo clippy, cargo fmt --check all pass
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verification: ast-grep shows the only remaining asset_id hits are the Godot Asset Library HTTP API field (godot/asset_lib.rs, commands/add.rs + search.rs, wiremock fixtures), which is intentionally not renamed. User-facing config error/panic strings updated to asset_library_id. Added unit test legacy_asset_id_alias_loads_and_round_trips: parses a legacy asset_id = 1216 config, asserts asset_library_id == Some(1216), and asserts re-serialization emits asset_library_id = 1216 with no asset_id. Full gate: cargo test all pass, cargo clippy --all-targets exit 0, cargo fmt --check exit 0.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Renamed the config field asset_id to asset_library_id with serde alias asset_id so legacy ggg.toml files still load (src/config.rs, src/dependency/lockfile.rs). Updated every call site (kind(), validate_source(), new_asset_lib, resolver/lockfile/update/ls_dep/deps/add, tests) including user-facing error/panic strings. Docs updated in docs/reference/configuration.md (field section, examples, legacy-alias note) and docs/reference/commands/add.md (example output); CHANGES.md notes the backwards-compatible rename under [Unreleased]. Verified with the new backwards-compat round-trip test and a full cargo test / clippy / fmt pass.
<!-- SECTION:FINAL_SUMMARY:END -->
