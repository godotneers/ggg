---
id: TASK-5.4
title: Asset Store dependency pipeline (resolve/download/cache/lock/sync)
status: Done
assignee: []
created_date: '2026-09-14 07:00'
updated_date: '2026-09-18 06:51'
labels: []
dependencies:
  - TASK-5.2
  - TASK-5.3
parent_task_id: TASK-5
priority: medium
type: task
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Wire the new dep kind through the whole dependency pipeline, mirroring the AssetLib path which already resolves a URL at sync time and downloads/hashes an archive. ggg.toml always pins a version (established in TASK-5.2; a version-less capture exists only as a CLI convenience in ggg add), so the resolver always resolves a pinned :version by version string (duplicate version strings resolve to the larger release id) against the asset's full release list. The project Godot version is threaded through resolve/ensure for store deps so the resolver can warn - and still install - when the pinned release's min/max_godot_version range does not cover the project's Godot version. Lock file gains publisher_slug, asset_slug, release_id, release_version next to the reused url + archive_sha. download/cache reuse the archive mechanics (resolved_url required, default strip_components = 1, configurable).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ResolvedDependency gains release id + release version fields; LockEntry gains publisher_slug/asset_slug/release_id/release_version beside url + archive_sha; existing asset library asset_version untouched
- [x] #2 The project Godot version is threaded through resolve/ensure for store deps and is used to warn when the pinned release's min/max Godot range does not cover the project's Godot version
- [x] #3 A pinned :version is resolved by version string (the only supported resolution strategy - ggg.toml always pins a version); duplicate version strings resolve to the larger release id; an incompatible pinned release installs with a clear warning
- [x] #4 download.rs and cache.rs handle AssetStore like AssetLib: resolved_url required, archive download with locally computed sha256, cache keyed by download url hash + sha, strip_components default 1 and configurable
- [x] #5 ggg sync round-trips over wiremock (resolve -> download -> cache -> lock -> install) writing the store lock entry and installing files; ggg ls-dep shows the store version header (publisher/asset vX.Y.Z)
- [x] #6 Integration tests following tests/sync.rs patterns; cargo test, cargo clippy, cargo fmt --check all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add release_id/release_version to ResolvedDependency (src/dependency/mod.rs) and fix every construction site (ast-grep 'ResolvedDependency {'). 2. Resolver (src/dependency/resolver.rs): thread Option<&GodotVersion> through resolve_dependency; AssetStore branch = locked_store hit or full get_releases(None, false) + select_pinned_release (largest id on duplicate version strings) + warn when min/max does not cover the project Godot version; extract pure select_pinned_release/is_compatible helpers + tolerant min/max parse + unit tests. 3. Ensure (src/dependency/ensure.rs): thread godot_version through ensure_dependency. 4. Lockfile (src/dependency/lockfile.rs): LockEntry store fields, upsert AssetStore branch, locked_store(name, publisher_slug, asset_slug, version) key; unit tests. 5. Commands: sync::plan passes Some(&config.project.godot.version); ls_dep header becomes 'publisher/asset v<release_version> -> <sha8>...' + pass version. 6. Test harness: TestProject::env_store_api helper. 7. Integration tests in tests/sync.rs (install, duplicate version -> larger id, incompatible pinned warns but installs, modified-owned force parity, version remap invalidates lock) and tests/ls_dep.rs (store header). 8. cargo test, cargo clippy --all-targets, cargo fmt --check; finalize per backlog finalization guide.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation notes: LockEntry store fields (publisher_slug, asset_slug, release_id, release_version) are Option-bearing with skip_serializing_if so pre-existing ggg.lock files stay compatible. Store lock key includes the pinned version string, so editing :version in ggg.toml invalidates the lock entry exactly like a git rev change. 'secret' values: the sync test needs GGG_ASSET_STORE_API_URL_ENV_VAR pointed at the mock server root (api.base_url()); the store client appends /releases/... itself. Integration data: sync_store_incompatible_pinned_warns_but_installs asserts stderr contains 'requires Godot v4.4..latest' (min 4.4, unbounded max); make_release ids chosen non-contiguous (10 vs 7) so duplicate-version resolution is proven by release_id, not by zip order. All 340 tests green.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented the Asset Store dependency pipeline (resolve -> download -> cache -> lock -> install) mirroring the AssetLib path. ResolvedDependency gained release_id/release_version; LockEntry gained publisher_slug/asset_slug/release_id/release_version with a version-aware lock key. The project Godot version is threaded through resolve_dependency/ensure_dependency; the resolver matches a pinned :version by version string (largest release id wins on duplicates) and warns-but-installs when min/max_godot_version does not cover the project version (parse_tolerant bounds). download/cache handle AssetStore like AssetLib (resolved_url required, sha256 computed locally, cache keyed by url hash + sha). ls-dep shows 'publisher/asset v<version> -> <sha8>...'. Verified by wiremock integration tests in tests/sync.rs (install round-trip, duplicate version -> larger release_id, incompatible pinned warns but installs, modified-own blocks then --force restores, :version remap removes stale files) and tests/ls_dep.rs (store header); 340 tests pass (255 lib + integration), cargo clippy --all-targets clean, cargo fmt --check clean. Docs/CHANGES.md deferred to TASK-5.8.
<!-- SECTION:FINAL_SUMMARY:END -->
