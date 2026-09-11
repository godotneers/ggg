---
id: TASK-1.11
title: Godot engine download via full mocked chain
status: Done
assignee: []
created_date: '2026-09-08 05:28'
updated_date: '2026-09-11 05:11'
labels: []
dependencies:
  - TASK-1.3
parent_task_id: TASK-1
type: task
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend wiremock.rs MockApi to mount the GitHub builds API (releases/tags/{tag}) plus a fake platform godot zip; sync downloads and installs the engine end-to-end with no real network. Also cover export-templates download via GGG_GODOT_DOWNLOADS_BASE.
Real edit/run launches remain mock-binary/interactive and are out of scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 MockApi mounts the builds API releases/tags/{tag} endpoint and a fake godot zip
- [x] #2 sync downloads and installs the engine end-to-end with no real network
- [x] #3 Export-templates download works via GGG_GODOT_DOWNLOADS_BASE
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add GGG_GODOT_DATA_DIR env var override in src/godot/export_templates.rs (godot_templates_dir fn), following the GGG_CACHE_DIR pattern. Document in docs/content/docs/reference/environment.md.

2. Extend MockApi in tests/common/wiremock.rs:
   - New types: ReleaseAsset, GodotReleaseBody for the GitHub releases API JSON
   - mount_builds_api(tag, assets: Vec<(String, Vec<u8>)>) — lower-level: serves builds API JSON at GET /releases/tags/{tag}, each (name, zip_bytes) gets a route with browser_download_url
   - mount_godot_release(release) — high-level convenience: calls fake_godot_zip + mount_builds_api for current platform
   - mount_export_templates(tpz_bytes) — serves .tpz zip at GET /

3. Add helpers in tests/common/archive.rs:
   - fake_godot_zip(release) — zip with fake Godot executable matching current platform naming
   - fake_template_tpz(release) — zip with templates/version.txt

4. Extend TestProject in tests/common/mod.rs:
   - env_builds_api(api) — sets GGG_GODOT_BUILDS_API_URL
   - env_downloads_base(api) — sets GGG_GODOT_DOWNLOADS_BASE_URL
   - env_data_dir(dir) — sets GGG_GODOT_DATA_DIR

5. Add write_no_seed() to ConfigBuilder — writes ggg.toml without seeding Godot cache

6. CLI test in tests/sync.rs: sync_downloads_engine_end_to_end (AC #1+#2)
   - MockApi with manifest + mount_godot_release, project with env_api + env_builds_api, write_no_seed, run ggg sync, assert executable in cache, assert received_requests

7. CLI test in tests/sync.rs: sync_downloads_export_templates (AC #3)
   - MockApi with manifest + mount_godot_release + mount_export_templates, project with env_api + env_builds_api + env_downloads_base + env_data_dir(tempdir), config with export_templates=true, write_no_seed, run ggg sync, assert version.txt in temp data dir

8. Run cargo test, cargo clippy, cargo fmt --check
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented:
- Added GGG_GODOT_DATA_DIR env var override in godot_templates_dir (export_templates.rs) + unit tests; documented in docs/content/docs/reference/environment.md
- MockApi: added ReleaseAsset/GodotReleaseBody types, mount_builds_api, mount_godot_release, mount_export_templates
- archive.rs: added fake_godot_asset, fake_template_tpz
- TestProject: env_builds_api, env_downloads_base, env_data_dir, cache_dir accessor
- ConfigBuilder: export_templates() builder + write_no_seed()
- sync.rs: 3 new CLI tests (engine end-to-end, exact-asset mock, export templates)
- cargo test (300 tests), cargo clippy --all-targets, cargo fmt --check all pass

Validation: cargo test (300 tests: 228 unit + 3 wiremock + 19 sync incl. 3 new), cargo clippy --all-targets clean, cargo fmt --check clean.

AC evidence:
- #1: sync_downloads_engine_end_to_end asserts received_requests includes GET /releases/tags/4.3-stable; sync_downloads_engine_from_mocked_release_with_exact_asset exercises mount_builds_api with multiple assets.
- #2: sync_downloads_engine_end_to_end with write_no_seed (empty cache) asserts {cache}/godot/4.3-stable contains a Godot executable, all over mock.
- #3: sync_downloads_export_templates sets GGG_GODOT_DOWNLOADS_BASE_URL, asserts version.txt installed in temp data dir and request with slug=export_templates.tpz received.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
- Extended the wiremock MockApi with the GitHub builds API: mount_builds_api (serves releases/tags/{tag} JSON plus a per-asset file route) and mount_godot_release convenience wrapper; plus the export-templates endpoint via mount_export_templates.
- Added fake_godot_asset/fake_template_tpz archive helpers.
- Added GGG_GODOT_DATA_DIR env override for install-location isolation (documented in environment.md).
- Added TestProject env_builds_api/env_downloads_base/env_data_dir helpers and ConfigBuilder export_templates()/write_no_seed().
- Three new CLI tests prove: engine download + install end-to-end (empty cache, no real network), exact-asset selection against a multi-asset release, and export-template install via GGG_GODOT_DOWNLOADS_BASE.
- Verified: cargo test (300 passing), cargo clippy --all-targets, cargo fmt --check clean.
<!-- SECTION:FINAL_SUMMARY:END -->
