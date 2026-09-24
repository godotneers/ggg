---
id: TASK-5.2
title: 'Add asset_store_asset config field and Source::AssetStore'
status: Done
assignee: []
created_date: '2026-09-14 06:59'
updated_date: '2026-09-18 05:24'
labels: []
dependencies:
  - TASK-5.1
parent_task_id: TASK-5
priority: medium
type: task
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Introduce the config surface for the Asset Store without any networking. Store deps are declared with a single field asset_store_asset = "publisher_slug/asset_slug:version" using docker/gradle-style tag syntax. Slugs are restricted to [a-z0-9-]{3,256} (matching the Asset Store frontend url_slug constraints); the version follows docker-tag syntax [A-Za-z0-9][A-Za-z0-9._-]* and is required, so store deps always pin a release.

The config types are implemented with the owned Source enum (each variant carries exactly the fields that apply to it), which replaces the earlier borrowed DepKind design: Dependency::source is a flattened Source, accessors are the enum public fields, and invalid field combinations are rejected at parse time rather than by a separate validation pass. ggg deps renders the new kind. The slug grammar must not collide with SSH/scp git URLs (which contain dots in the host) or numeric asset library ids.

Resolving a store dep to a download is out of scope here (that is TASK-5.4); the resolver rejects store deps until then.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Dependency gains `asset_store_asset` parsed as `publisher_slug/asset_slug:version` into Source::AssetStore { asset_store_asset: AssetStoreRef, strip_components }; empty or malformed specs are rejected with clear errors via AssetStoreRef::from_str at parse time
- [x] #2 Source::AssetStore exposes parsed publisher/asset/version fields; Dependency::source treats asset store as a distinct source and deserialisation enforces exactly one of git/url/asset_library_id/asset_store_asset set
- [x] #3 Single Dependency::new(name, source, map, exclude) constructor replaces the per-source helpers; ggg.toml round-trip preserved via toml_edit
- [x] #4 ggg deps renders store deps as type asset-store showing publisher_slug/asset_slug:version (asset library deps render as asset-lib)
- [x] #5 Unit tests: valid specs, malformed specs (bad slug chars/length, bad version tag), SSH-style git URLs still classify as git, numeric ids still asset library
- [x] #6 docs/reference/configuration.md + CHANGES.md updated; cargo test, cargo clippy, cargo fmt --check pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Update task metadata to match agreed design (Source enum, required version, 3-256 slug length)
2. Verify/polish AssetStoreRef parsing + Source validation (already uncommitted on asset_store_integration)
3. Add unit tests in src/config.rs: valid/malformed specs, config parsing of asset_store_asset, source exclusivity, SSH-URL->git, numeric->asset lib, toml_edit round-trip
4. Add ConfigBuilder::asset_store + tests/deps.rs integration test asserting asset-store render
5. Update CHANGES.md, docs/reference/configuration.md, docs/reference/commands/deps.md
6. Run cargo test, cargo clippy, cargo fmt --check; fix fallout
7. Finalize per backlog finalization guide
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the Source enum refactor (DepKind removed) plus AssetStoreRef parsing on top of the pre-existing uncommitted work on asset_store_integration. Requirements, per user decisions: version in asset_store_asset = publisher_slug/asset_slug:version is mandatory; slugs are [a-z0-9-]{3,256}; version is docker-tag style [A-Za-z0-9][A-Za-z0-9._-]*. Task description/ACs updated to match.

Key implementation detail: configuration errors now surface at parse time (custom Source::deserialize -> RawSource + AssetStoreRef::from_str), and validate_source() only re-checks archive URL extensions for Rust-constructed deps. AssetStoreRef::from_str uses anyhow Context/map_err so the top-level error string includes the specific slug/version reason.

Validation: cargo test (14 suites, incl. new config unit tests + deps_lists_asset_store_dependency integration test) all pass; cargo clippy --all-targets clean; cargo fmt --check clean.

Review feedback applied after initial finalization: (1) replaced the four per-source constructors (new_git/new_archive/new_asset_lib/new_asset_store) with a single Dependency::new(name, source, map, exclude); (2) renamed ConfigBuilder::asset to asset_lib and updated all test call sites; (3) ggg deps now labels asset library deps as asset-lib (was asset) and the docs example was corrected to the real render format (asset #<id>). Re-verified: cargo test 14 suites green, cargo clippy --all-targets clean, cargo fmt --check clean, doctest for Dependency::new passes.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Introduced the Asset Store config surface (no networking, per scope). ggg.toml now accepts asset_store_asset = "publisher_slug/asset_slug:version" as a fourth dependency source.

Delivered on the pre-existing uncommitted Source-enum refactor (owned Source replaces borrowed DepKind, flattened into Dependency::source):
- AssetStoreRef parses publisher/asset/version with slugs [a-z0-9-]{3,256} and docker-tag versions; FromStr/Display/Serialize/Deserialize + clear errors.
- Source::deserialize collects all candidate fields then rejects any combo other than exactly one of git/url/asset_library_id/asset_store_asset; misplaced aux fields (rev/sha256) rejected; strip_components allowed on archive/asset-lib/asset-store.
- Single Dependency::new(name, source, map, exclude) constructor (per-source helpers removed); toml_edit round-trip preserved (serializes asset_store_asset = "pub/asset:ver").
- ggg deps renders type asset-store with publisher_slug/asset_slug:version; asset library deps render as asset-lib. Resolver bails for store deps (fully wired in TASK-5.4).
- Pipeline modules migrated from DepKind to Source.

Docs: CHANGES.md, reference/configuration.md (four kinds, asset_store_asset field, strip_components default), reference/commands/deps.md, docs _index.md.

Decisions locked in via review: version mandatory in the config field; slug length 3-256 per the Store frontend constraint; keep the Source enum over DepKind; single constructor over per-source helpers; asset-lib type label. Task description/ACs reworded to match.

Verified with: cargo test (14 suites green, including 11 config unit tests and tests/deps.rs deps tests), cargo clippy --all-targets clean, cargo fmt --check clean. All 6 acceptance criteria checked.
<!-- SECTION:FINAL_SUMMARY:END -->
