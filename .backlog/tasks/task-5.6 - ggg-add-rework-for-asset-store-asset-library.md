---
id: TASK-5.6
title: ggg add rework for asset-store/asset-library
status: Done
assignee: []
created_date: '2026-09-14 07:00'
updated_date: '2026-09-20 12:55'
labels: []
dependencies:
  - TASK-5.2
  - TASK-5.3
parent_task_id: TASK-5
priority: medium
type: task
ordinal: 26000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rework ggg add to cover both sources. Source keywords become git, archive, asset-store, asset-library where asset becomes an alias of asset-store (breaking change - previously it was the asset library). The bare form ggg add <input> auto-detects: pure number -> asset_library_id; a publisher/slug[:version] spec matching the strict grammar (from TASK-5.2) -> Asset Store; any other keyword -> a combined search across both the store and the library with results visibly prefixed by source; git/archive URL routing is preserved with the store grammar checked first so SSH/scp URLs (containing dots) are never mistaken for store specs. Release selection: bare specs pick the latest stable Godot-compatible release, spec+version or --version pin the exact version, ambiguous compatible releases are offered through an interactive picker, and the asset-library path keeps its --id option.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Source keywords: asset-store, asset-library, and asset as an alias of asset-store (breaking, recorded in CHANGES.md); git/archive unchanged, and archive now derives its default name from the URL filename so --yes works without --name
- [x] #2 Bare form auto-detects: pure number -> asset_library_id; a publisher/slug[:version] spec matching the strict grammar -> Asset Store; any other keyword -> a combined store+library search with results visibly prefixed by source; git/archive URL routing is preserved with the store grammar checked first so SSH/scp URLs (containing dots) are never mistaken for store specs
- [x] #3 Store add auto-picks the latest stable Godot-compatible release for bare specs (largest release id among stable releases whose min/max Godot range covers the project version), pins the exact version for spec:version or --version, and adds an incompatible pinned release with a clear warning; no interactive release picker
- [x] #4 The asset-library path keeps its --id option; result handling is unified across store-only, library-only, and combined searches: 0 results -> error, 1 -> auto-add with confirmation, 2-5 -> interactive picker with Cancel and source prefixes, 6+ -> error suggesting ggg search
- [x] #5 Wiremock integration tests: numeric id, bare spec, spec+version, --version, asset alias -> store, keyword combined search (single hit and 6+ totals), explicit asset-library add, archive --yes filename naming, and the --id-under-store guidance error; the 2-5 picker itself stays untested pending TASK-5.9; CHANGES.md records the breaking asset alias change
- [x] #6 cargo test, cargo clippy --all-targets, cargo fmt --check all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Config (src/config.rs): export validate_slug/validate_version_tag as pub(crate) so add.rs can detect store specs without a version.
2. Resolver (src/dependency/resolver.rs): make select_pinned_release/is_compatible/parse_tolerant pub(crate); add select_latest_stable_compatible (largest id among stable && is_compatible) + unit tests.
3. CLI (src/main.rs): AddArgs gains --version (long-only, Asset Store Options heading); dispatch asset-library -> run_asset, asset/asset-store -> run_asset_store, bare -> run_bare; validate --id (asset-library only), --version (store-routed only), --sha256 (archive only); update the Add doc comment.
4. commands/add.rs: run_asset_store(spec|keyword, version, name, yes, strip_components) via get_asset + full get_releases; pinned -> select_pinned_release, bare -> select_latest_stable_compatible; incompatible pinned warns but adds; numeric/--id under store -> guidance error. run_bare rework: archive ext -> archive (default name from URL filename), store grammar -> store, git/URL -> git, pure number -> asset library id, else -> combined search. Shared disambiguate(): 0 error / 1 auto-add / 2-5 interactive Select with source prefixes + Cancel / 6+ error suggesting ggg search, used by run_asset, store keyword search, and run_combined. Helpers infer_name_from_archive_url + looks_like_store_spec + unit tests.
5. Integration tests (tests/add.rs): existing asset keyword test -> asset-library; new wiremock tests: numeric id bare, bare store spec -> latest stable, spec:version, --version flag, asset alias -> store, --id under store errors with guidance, combined single-hit auto-add, combined 6+ error, explicit asset-library --id, archive --yes filename naming. The 2-5 picker stays untested pending TASK-5.9.
6. Docs & changelog: full rewrite of docs/reference/commands/add.md (current behaviour only), CHANGES.md breaking entry for the asset alias, fix stale "ggg add asset" references in configuration.md/update.md/update command help.
7. Verify: cargo test, cargo clippy --all-targets, cargo fmt --check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented per plan. Validation passed: cargo test all suites green (275 unit incl. new select_latest_stable_compatible x6 and add.rs helper x12, 22 add integration incl. 11 new wiremock tests), cargo clippy --all-targets clean, cargo fmt --check clean. Behavior: asset now aliases asset-store; bare supplier/slug pins latest stable-compatible (largest id among stable && is_compatible); spec:version / --version pin exact (incompatible pinned warns but adds); --id is asset-library-only; bare numeric -> library id; bare plain query -> combined store+library search through shared disambiguate (0 error / 1 auto-add / 2-5 picker with [store]/[library] prefixes + Cancel / 6+ error suggesting ggg search). Archive derives default name from URL filename (--yes works without --name). Docs: add.md fully rewritten, CHANGES.md breaking entry for asset alias, stale ggg add asset refs fixed in configuration.md/update.md/update help. StoreRelease gained Clone for release resolution. Picker (2-5) remains untested -> TASK-5.9.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Reworked ggg add for asset-store/asset-library: asset-store keyword (alias asset), asset-library keeps --id, bare auto-detect (number->library id, strict supplier/slug[:version]->store first so SCP/SSH URLs never collide, plain query->combined store+library search), unified disambiguate (0 error / 1 auto-add / 2-5 picker with source prefixes / 6+ error), store adds always pin a version (bare->latest stable Godot-compatible; :version/--version exact; incompatible pinned warns but adds), archive --yes now works without --name via URL-filename inference. Verified with cargo test (275 unit + 22 add integration green), cargo clippy --all-targets, and cargo fmt --check.
<!-- SECTION:FINAL_SUMMARY:END -->
