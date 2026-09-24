---
id: TASK-5.5
title: ggg search --source (default asset-store)
status: Done
assignee: []
created_date: '2026-09-14 07:00'
updated_date: '2026-09-20 09:46'
labels: []
dependencies:
  - TASK-5.3
parent_task_id: TASK-5
priority: medium
type: task
ordinal: 25000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend ggg search with a --source flag whose default flips from the asset library to the Asset Store - a breaking change to call out in CHANGES.md, since the store is the future home for Godot addons and the asset library is slated for removal. Sources: asset-store (default) and asset-library. Each source keeps its own result columns and hint line. Both send the project Godot version (major.minor) for compatibility filtering: the library via godot_version, the store via the compatibility param established in the client subtask.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ggg search gains --source accepting asset-store (default) and asset-library only; invalid values are rejected
- [x] #2 asset-library source keeps the existing ID/Title/Author/License columns, godot version filter override, and hint line referencing ggg add asset-library
- [x] #3 asset-store source shows Publisher/Name/License columns, sends type=0 + the compatibility param from the project/discovered Godot version, and its hint line references ggg add
- [x] #4 CHANGES.md records the breaking default flip from asset-library to asset-store; docs/reference/commands/search.md updated
- [x] #5 Wiremock integration tests for both sources; cargo test, cargo clippy, cargo fmt --check all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. CLI: add trailing `source: SearchSource` arg to Command::Search in src/main.rs; define SearchSource (clap ValueEnum: asset-store = default, asset-library) in src/commands/search.rs; update the Search doc comment and dispatch to run(.., source).
2. commands/search.rs: run(query, godot_version_override, source). AssetLibrary keeps asset_lib::search + ID/Title/Author/License columns, hint "Use `ggg add asset-library --id <N>` to add a specific asset."; AssetStore uses asset_store::search (type=0 + compatibility already in client) + Publisher/Name/License columns (Publisher = publisher.name), hint "Use `ggg add asset-store <publisher>/<slug>:<version>` to add a specific asset.". Share no-results/version-label/Showing X of Y logic; per-source error context; update module doc.
3. tests/search.rs: add `--source asset-library` to the 6 existing tests; add a store request-param helper (type + compatibility); new tests: default source hits store (columns/hint/type=0/compatibility=4.3), explicit --source asset-store, store without ggg.toml (no compatibility, no "on Godot"), store "Showing X of Y", invalid --source value rejected (clap). Use env_store_api + mount_store_search.
4. docs: update docs/content/docs/reference/commands/search.md (--source flag, per-source columns/hints, breaking default, refresh stale example); add Changed entry to CHANGES.md for the breaking default flip.
5. Verify: cargo test, cargo clippy --all-targets, cargo fmt --check.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented. CLI: Command::Search gains `--source <asset-store|asset-library>` via clap ValueEnum (SearchSource in src/commands/search.rs), default asset-store; dispatch passes source to commands::search::run. run() resolves the Godot version as before, then routes per source: asset_store::search -> Publisher/Name/License table + `ggg add asset-store <publisher>/<slug>:<version>` hint; asset_lib::search -> ID/Title/Author/License + `ggg add asset-library --id <N>` hint; shared no-results/version-label/Showing-X-of-Y footer extracted to print_result_footer. Tests: 6 existing search tests now pass `--source asset-library`; 5 new: default source is store (columns/hint/type=0/compatibility=4.3 asserted via sent_store_search_params), explicit --source asset-store, store without ggg.toml (no compatibility, no on-Godot label), store truncation message, invalid --source rejected by clap. Docs: search.md rewritten (flag, per-source output, fresh example), CHANGES.md Changed entry records the breaking default flip.

Validation passed (cargo test 14 suites green, incl. 11 search.rs round-trips; cargo clippy --all-targets clean; cargo fmt --check clean). Library hint line assertion added to ensure AC#2 wording is covered by an integration test.

Review changes (2026-09-20): search.rs restructured per review - run() now owns the version label and prints the no-results message, the shared footer (print_result_footer), and the hint; the per-source helpers return a SearchOutcome { total, shown, hint } and only print their own table, so footer/no-results logic is no longer duplicated. The store hint is now the bare form "ggg add asset-store <publisher>/<slug>" (verified against the live Asset Store API: search hits and get_asset carry no version - only get_releases does - so no Version column; a bare add resolves the latest stable Godot-compatible release per TASK-5.6). docs search.md updated accordingly (bare hint + note that search output shows no version). Validation re-run: cargo test all suites green (255 unit + 11 search + others), cargo clippy --all-targets clean, cargo fmt --check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
ggg search gains a --source flag whose default flips from the Asset Library to the Asset Store - a breaking change recorded in CHANGES.md. New SearchSource (clap ValueEnum) in src/commands/search.rs accepts asset-store (default) and asset-library; invalid values are rejected by clap. search::run routes per source: the store renders Publisher/Name/License via the asset_store client (sending type=0 + the MAJOR.MINOR compatibility param from the project/--godot-version Godot version) and hints at `ggg add asset-store <publisher>/<slug>:<version>`; the library keeps its existing ID/Title/Author/License columns, godot_version filter, and now hints at `ggg add asset-library --id <N>`. No-results/version-label/truncation footer shared via print_result_footer.

Docs: docs/reference/commands/search.md rewritten for --source and per-source output with a fresh example; CHANGES.md documents the breaking default flip under Changed.

Verification: tests/search.rs updated (6 library tests pass --source asset-library explicitly) plus 5 new store/cli tests (default-source is store with outgoing type=0 + compatibility=4.3 asserted via received_requests; explicit --source asset-store; store without ggg.toml omits compatibility; store truncation; invalid source rejected). cargo test 14 suites green, cargo clippy --all-targets clean, cargo fmt --check clean. All 5 acceptance criteria checked.
<!-- SECTION:FINAL_SUMMARY:END -->
