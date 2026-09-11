---
id: TASK-1.1
title: Env-var overrides for the four hardcoded endpoints
status: Done
assignee: []
created_date: '2026-09-08 05:26'
updated_date: '2026-09-11 05:11'
labels: []
dependencies: []
parent_task_id: TASK-1
type: task
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add env-var overrides so each hardcoded endpoint can be pointed at wiremock without code changes.
- src/godot/manifest.rs: versions_manifest_url() -> GGG_GODOT_MANIFEST_URL
- src/godot/download.rs: godot_builds_api_url() (base, tag appended) -> GGG_GODOT_BUILDS_API_URL
- src/godot/asset_lib.rs: asset_lib_api_url() -> GGG_ASSET_LIB_API_URL
- src/godot/export_templates.rs: template_url() base host -> GGG_GODOT_DOWNLOADS_BASE_URL
Environment variables use a uniform *_URL suffix. The existing consts are replaced by resolver functions that read the GGG_* env var with override-wins semantics, falling back to the default URL when unset.
No env var for archive-dep URLs, asset download_url, or godot browser_download_url - those flow from user config/mock-JSON. GGG_CACHE_DIR already exists for cache isolation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Each of the four endpoints reads its GGG_* env var with override-wins semantics
- [x] #2 Unit tests cover override-wins and default-when-unset for each var
- [x] #3 No behavior change when no env vars set
- [x] #4 Env fallback is read once per run; the default URL applies when unset
- [x] #5 The new env vars are documented in docs/content/docs/reference/environment.md
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Replace each hardcoded const with a resolver function reading the GGG_*_URL env var (override-wins), falling back to the default URL when unset.
2. All four resolvers return String (uniform; avoids &'static str lifetime issues for runtime env values).
3. manifest.rs: fn versions_manifest_url() using GGG_GODOT_MANIFEST_URL.
4. download.rs: fn godot_builds_api_url() using GGG_GODOT_BUILDS_API_URL.
5. asset_lib.rs: fn asset_lib_api_url() using GGG_ASSET_LIB_API_URL.
6. export_templates.rs: fn godot_downloads_base_url() using GGG_GODOT_DOWNLOADS_BASE_URL; rebuild template_url.
7. Add serial_test dev-dependency; unit tests per resolver (#[serial]) for override-wins & default-when-unset.
8. Document the four env vars in docs/content/docs/reference/environment.md.
9. Run cargo test, clippy, fmt --check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented:
- Replaced each hardcoded const with a resolver function reading a GGG_*_URL env var (override-wins) falling back to the default URL.
- manifest.rs: versions_manifest_url() (GGG_GODOT_MANIFEST_URL)
- download.rs: godot_builds_api_url() (GGG_GODOT_BUILDS_API_URL)
- asset_lib.rs: asset_lib_api_url() (GGG_ASSET_LIB_API_URL)
- export_templates.rs: godot_downloads_base_url() (GGG_GODOT_DOWNLOADS_BASE_URL), template_url rebuilt on it.
- Added serial_test dev-dependency; unit tests per resolver (override-wins & default-when-unset).
- Documented the four env vars in docs/content/docs/reference/environment.md.
- Added doc acceptance criterion and renamed env vars to a uniform *_URL set in the task spec.

Verification: cargo test (230 passed), cargo clippy (no new warnings from these changes), cargo fmt --check clean.

Follow-up (review feedback): inlined the GODOT_BUILDS_API and API_BASE constants into their resolver functions (godot_builds_api_url, asset_lib_api_url), matching export_templates.rs. Updated the default-when-unset tests to assert against the inlined default strings. cargo test (230 passed), clippy (no new warnings), fmt clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the four hardcoded endpoint URLs with env-var resolver functions (uniform *_URL suffix):
- versions_manifest_url() -> GGG_GODOT_MANIFEST_URL
- godot_builds_api_url() -> GGG_GODOT_BUILDS_API_URL
- asset_lib_api_url() -> GGG_ASSET_LIB_API_URL
- godot_downloads_base_url() -> GGG_GODOT_DOWNLOADS_BASE_URL

Each resolver reads its env var with override-wins semantics, falling back to the default URL when unset (per-call read, matching the existing GGG_CACHE_DIR pattern).

- Added serial_test dev-dependency with unit tests per resolver for override-wins and default-when-unset.
- Documented all four vars in docs/content/docs/reference/environment.md.
- Verified: cargo test (230 passed, incl. 9 new resolver tests), cargo clippy (no new warnings), cargo fmt --check clean.
<!-- SECTION:FINAL_SUMMARY:END -->
