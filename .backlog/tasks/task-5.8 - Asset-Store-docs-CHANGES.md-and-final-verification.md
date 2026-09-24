---
id: TASK-5.8
title: 'Asset Store docs, CHANGES.md, and final verification'
status: Done
assignee: []
created_date: '2026-09-14 07:01'
updated_date: '2026-09-24 06:56'
labels: []
dependencies:
  - TASK-5.4
  - TASK-5.5
  - TASK-5.6
  - TASK-5.7
modified_files:
  - CHANGES.md
  - docs/content/docs/reference/commands/deps.md
parent_task_id: TASK-5
priority: medium
type: docs
ordinal: 28000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Close-out subtask for the whole feature. Ensures all user-facing docs reflect the Asset Store integration and the breaking changes, CHANGES.md tells the migration story, and the project passes its full verification suite plus a live smoking of the flow the individual wiremock tests cannot cover. Depends on every earlier subtask.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 docs/reference/configuration.md documents asset_library_id (legacy asset_id alias) and asset_store_asset = "publisher/slug[:version]" incl. version selection semantics and the strip_components default of 1 for store deps
- [x] #2 Command docs (add, search, update, deps) reflect the new keywords, --source flag, aliases, and source-prefixed combined search; docs/reference/environment.md lists GGG_ASSET_STORE_API_URL
- [x] #3 CHANGES.md captures the new feature and both breaking changes (search default flip to asset-store, asset add alias flip) with migration guidance
- [x] #4 cargo test, cargo clippy, cargo fmt --check all green
- [x] #5 Manual smoke test of ggg add / ggg search / ggg sync against the live Asset Store performed; results and any discrepancies captured in the final summary
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. CHANGES.md [Unreleased] cleanup: remove the internal "shared newest logic" Added entry; tighten Fixed#1 (update lock/config drift abort) and Fixed#2 (sync lock cleanup) to user-visible impact; drop the semantic-version/tie-break parenthetical from the ggg update Added entry.
2. Docs coherence pass for AC1/AC2: fix deps.md duplicated sentence; verify configuration.md (asset_library_id legacy alias, asset_store_asset spec semantics, strip_components default 1), add.md (keywords, asset alias, bare-detection table, source-prefixed combined search), search.md (--source default), update.md, environment.md (GGG_ASSET_STORE_API_URL) against actual CLI behaviour; fix drift. cache.md out of scope.
3. Final verification (AC4): cargo test, cargo clippy --all-targets, cargo fmt --check.
4. Live smoke test (AC5) against real store API: temp scratch project with fake-seeded Godot cache and GGG_CACHE_DIR/GGG_GODOT_DATA_DIR overrides; run ggg search --source asset-store, ggg add asset-store <publisher/slug>, ggg sync; capture output and discrepancies.
5. Finalize: check acceptance criteria with evidence across 1-4, write final summary including smoke-test results.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Docs/CHANGES coherence pass complete.
- CHANGES.md [Unreleased]: removed the internal "shared newest logic" entry; tightened the ggg update Added entry (dropped semantic-version/tie-break parenthetical), Fixed#1 (update abort wording, dropped "unchocked" typo and validation jargon) and Fixed#2 (sync lock cleanup in user-visible terms).
- deps.md: removed duplicated sentence ("...spec is shown." x2).
- Coherence verified against code: strip_components defaults (Archive 0, AssetLib/AssetStore 1 at resolution, src/dependency/mod.rs:168), search default + --source (main.rs:106, search.rs), add asset alias + asset-library/--id (main.rs, add.rs), update lib+store logic (update.rs), deps labels asset-lib/asset-store (deps.rs), configuration.md spec/alias/strip_components claims all match code.

Verification: cargo test 419 passed (302 unit + integration + wiremock + 2 doc); cargo clippy --all-targets clean; cargo fmt --check clean.

AC5 live smoke test (temp project, fake-seeded Godot cache, real GGG_ASSET_STORE_API_URL default):
- ggg search gut -> 24 results shown of 371, correct footer/hint. Default source confirmed = Asset Store.
- ggg add asset-store souleat/godot-xoshiro256-plus-plus --yes -> pinned v0.1.0; ggg.toml wrote asset_store_asset = "souleat/godot-xoshiro256-plus-plus:0.1.0" (no strip_components field; default 1 applies at resolution).
- ggg sync -> fetched/downloaded from live store, installed 13 files under addons/GodotXoshiro256PP (strip_components=1 correct), wrote ggg.lock (url/archive_sha/publisher_slug/asset_slug/release_id/release_version), cached under cache/deps/<sha256(url)>, appended .ggg.state to .gitignore.
- Re-sync -> "locked v0.1.0: up to date (13 files)" (idempotent via lock).
- ggg deps -> type shows asset-store, source shows publisher/slug:version.
- ggg search terrain --source asset-library -> works (legacy source intact).

Minor discrepancy: add.md example shows the doc asset resolving to v1.1.0, but the live store currently serves v0.1.0 as newest stable for Godot 4.3. Example behavior fully matched docs; only the illustrative version number is stale. Left as-is (would go stale again on next release).

Second streamlining pass on CHANGES.md [Unreleased] (2026-09-24): further trimmed per end-user focus. Breaking changes: removed "for as long as the Asset Library still exists", the search/add entries now state just the flip + migration path; deps entry reworded (asset-store/asset-lib). Added: asset-store entry condensed (dropped disambiguation-flow detail, combined-search internals, incompatible-pin warning); keyword-search fact split into its own one-liner; archive name-inference and ggg update entries trimmed (dropped comparison mechanics and file-touch details). Changed: condensed rename + rewrite-on-save. Fixed: both entries reduced to user-visible impact (dropped 4-case drift enumeration and lock-entry rewrite internals). No code changes; tests/clippy/fmt unaffected.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
TASK-5.8 (Asset Store close-out): all five ACs verified.

Changed: CHANGES.md [Unreleased] trimmed of internal-detail entries (removed the shared "newest logic" note; tightened ggg update Added, Fixed#1 and Fixed#2 to user-visible terms, fixed "unchocked" typo); deps.md duplicate sentence removed; coherence pass confirmed add/search/update/deps/configuration/environment docs match actual CLI + pipeline behaviour (defaults, aliases, --source, labels, env var).

Verified: cargo test 419 passed, cargo clippy --all-targets clean, cargo fmt --check clean.

Live smoke test (AC5) run end-to-end against the real Godot Asset Store in a temp project: search (default store source, correct hint), add asset-store pinned release written to ggg.toml, sync downloaded/installed 13 files with strip_components=1, wrote ggg.lock + cache entry + .ggg.state/.gitignore, re-sync idempotent, deps shows asset-store label, legacy asset-library search still works. Only discrepancy: add.md example version (v1.1.0) is newer than the live newest stable for that asset (v0.1.0) - illustrative only, not a behaviour mismatch.
<!-- SECTION:FINAL_SUMMARY:END -->
