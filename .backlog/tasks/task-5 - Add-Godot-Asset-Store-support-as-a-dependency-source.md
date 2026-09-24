---
id: TASK-5
title: Add Godot Asset Store support as a dependency source
status: Done
assignee: []
created_date: '2026-09-14 06:59'
updated_date: '2026-09-24 06:46'
labels: []
dependencies: []
priority: high
type: feature
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Godot Asset Store (store.godotengine.org, OpenAPI spec at https://store.godotengine.org/api/v1/openapi.json) is replacing the Godot Asset Library. ggg must support store assets as a first-class dependency source alongside git, archive, and asset library deps.

The Asset Store identifies assets as publisher_slug/asset_slug pairs and serves multiple releases per asset (each with an id, display version string, stability flag, min/max Godot version, and download URL). This enables the two capabilities the asset library never had: version pinning and Godot-version compatibility filtering.

Config stays backwards compatible: the asset library field is renamed asset_id -> asset_library_id (accepted via serde alias), and store deps use a single field asset_store_asset = "publisher_slug/asset_slug[:version]" (docker/gradle-style tag syntax). CLI surface is reworked: ggg search gains --source (default asset-store, breaking change) and ggg add gains asset-store/asset-library keywords with asset aliasing asset-store (breaking change). The dependency pipeline is extended for store sources: resolve -> download -> cache -> lock -> install, reusing the archive mechanics of the asset library path.

Breaking changes (to be called out in CHANGES.md): search default source flips to asset-store; ggg add asset now means asset-store.

Implied workflow: subtasks are ordered so each is mergeable and verifiable on its own (unit/integration tests + cargo test/clippy/fmt green) while preparing the plumbing for the next.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Asset store deps are a fully working dependency source: ggg add -> sync -> update -> remove round-trips against the Asset Store API
- [x] #2 Existing asset library deps keep working unchanged (config, search, add, update)
- [x] #3 All breaking changes (search default, add asset alias) are recorded in CHANGES.md
- [x] #4 cargo test, cargo clippy, cargo fmt --check are green across the merged subtasks
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Close-out (2026-09-24). All 11 subtasks Done.
- AC evidence: AC1 add/sync live smoke (TASK-5.8), update wiremock tests (TASK-5.7), remove is source-agnostic by-name removal (src/commands/remove.rs:13) + unit tests; AC2 asset library unchanged (TASK-5.1 legacy asset_id alias tests, TASK-5.3 wiremock round-trip, TASK-5.5 --source asset-library, TASK-5.8 live library search); AC3 breaking changes in CHANGES.md verified in TASK-5.8; AC4 cargo test 419 passed + clippy + fmt --check green at TASK-5.8 close-out.
- TASK-5.9 (interactive prompt testability spike, LOW) was de-scoped: contents carried verbatim into a new standalone task TASK-6; TASK-5.9 left as a resolved [superseded] stub so the parent closes with no open subtasks.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
All 11 subtasks (TASK-5.1 - TASK-5.11) are Done. Godot Asset Store is a first-class dependency source: config (asset_store_asset = "publisher/slug[:version]" + asset_id -> asset_library_id legacy alias), API client with GGG_ASSET_STORE_API_URL override, resolve/download/cache/lock/sync pipeline, ggg add asset-store (alias asset) / asset-library, ggg search --source (default asset-store), ggg update for store deps, lock/config drift handling, docs, CHANGES.md migration story and live smoke test. Verification: cargo test (419) + cargo clippy + cargo fmt --check green; live add/search/sync round-trip against the real Asset Store (TASK-5.8 AC5). Breaking changes (search default flip, add asset alias flip, deps asset-lib label) recorded in CHANGES.md. TASK-5.9 spike de-scoped to standalone TASK-6 (all subtasks under TASK-5 resolved).
<!-- SECTION:FINAL_SUMMARY:END -->
