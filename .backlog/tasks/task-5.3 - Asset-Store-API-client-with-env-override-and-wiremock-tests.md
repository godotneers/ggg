---
id: TASK-5.3
title: Asset Store API client with env override and wiremock tests
status: Done
assignee: []
created_date: '2026-09-14 07:00'
updated_date: '2026-09-18 06:16'
labels: []
dependencies:
  - TASK-5.2
parent_task_id: TASK-5
priority: medium
type: task
ordinal: 23000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
New client module godot/asset_store.rs modeled on the existing godot/asset_lib.rs. Talks to https://store.godotengine.org/api/v1 (overridable via a new env var for tests). Endpoints needed: GET /search/query/ (with query, type=0 for addons, compatibility for Godot filtering, stable filters, sort, pagination/scroll), GET /assets/{publisher}/{asset}/, GET /releases/{publisher}/{asset}/ (with stable_only + compatibility filters). Deserialize only the fields the pipeline actually needs and ignore everything else, since the upstream OpenAPI is partly inaccurate (e.g. ReleaseData.size is typed boolean). The exact format of the compatibility param (e.g. "4.3" vs "4.3-stable") must be confirmed against the live API and pinned down here; keep it wiremock-testable either way.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 godot/asset_store.rs exposes search(query, godot_version), get_asset(publisher, slug), get_releases(publisher, slug, godot_version, stable_only) modeled on asset_lib.rs
- [x] #2 envvars.rs adds GGG_ASSET_STORE_API_URL override wired through the client; docs/reference/environment.md updated
- [x] #3 Structs deserialize only the needed fields (slug, publisher name/slug, name, license_type, store_url, release id/version/stable/download_url/min_godot_version/max_godot_version); extra/unknown JSON is ignored, avoiding the upstream size boolean spec bug
- [x] #4 Search sends type=0 (addons) and the compatibility param; the exact compatibility format is confirmed against the live API during this subtask and documented
- [x] #5 Wiremock tests cover search, get_asset, get_releases incl. stable_only filtering and basic pagination, following tests/wiremock.rs patterns
- [x] #6 cargo test, cargo clippy, cargo fmt --check all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add GGG_ASSET_STORE_API_URL to envvars.rs (ASSET_STORE_API_URL_ENV_VAR) and register pub mod asset_store in godot/mod.rs
2. Create src/godot/asset_store.rs modeled on godot/asset_lib.rs: asset_store_api_url() reading the new env var (default https://store.godotengine.org/api/v1); StorePublisher {name, slug}; StoreAsset {slug, publisher, name, license_type, store_url} shared by search+get_asset; Release {id: u64, version, stable, download_url, min_godot_version, max_godot_version: Option<String>}; de_string_u32 for the string-typed `count`. Only needed fields are deserialised; unknown JSON (incl. the mis-typed ReleaseData.size) is ignored.
3. search(query, godot_version) -> (Vec<StoreAsset>, u32): GET /search/query/ with type=0, query, and compatibility only when godot_version non-empty (format MAJOR.MINOR e.g. 4.3 - confirmed against live API, 4.3-stable returns 422). First-page-only per user decision; total comes from the string `count` field. No scroll follow.
4. get_asset(publisher, slug) -> StoreAsset: GET /assets/{publisher}/{slug}/.
5. get_releases(publisher, slug, godot_version, stable_only) -> Vec<Release>: GET /releases/{publisher}/{slug}/ passing compatibility (when non-empty) and stable_only through to the server only - no client-side filtering per user decision; wiremock asserts the outgoing params.
6. #[serial] URL override/default unit tests in asset_store.rs mirroring asset_lib.rs.
7. tests/common/wiremock.rs: StoreAsset body, StoreSearchBody (string count, hits[{asset}], scroll, tag_filters), StoreReleaseBody (mixed stable/unstable, nullable max_godot_version, {base} download URLs) + MountApi::mount_store_search/mount_store_asset/mount_store_releases.
8. tests/wiremock.rs round-trips (spawn_blocking + env override): search asserting type=0 + compatibility=4.3 in outgoing request and (hits, total) with total>hits + scroll token present (basic pagination as "Showing X of Y"); get_asset parsing all 5 fields; get_releases asserting outgoing stable_only=true + compatibility=4.3 and parsed fields incl. Option max_godot_version.
9. docs/reference/environment.md: add GGG_ASSET_STORE_API_URL row to the endpoint-overrides table (default https://store.godotengine.org/api/v1).
10. Verify cargo test, cargo clippy --all-targets, cargo fmt --check; fix fallout.

11. Review revision: move de_string_u32/de_optional_hash into shared src/utils/de.rs (used by both asset_lib and asset_store); drop redundant #[serde(rename=...)] on StoreRelease min/max_godot_version (field name == wire name); change search/get_releases/asset_lib::search to take Option<&GodotVersion> and format MAJOR.MINOR internally via new GodotVersion::major_minor(); update callers (commands/search.rs resolves --godot-version via FromStr and default from ggg.toml as a GodotVersion, commands/add.rs, tests/wiremock.rs); s/deserialise/deserialize/ across config.rs, asset_lib.rs, asset_store.rs, utils/de.rs.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
API facts confirmed against the live store (2026-09-18):
- compatibility accepts MAJOR.MINOR ("4.3"); "4.3-stable" -> 422 "use semantic version in the form of MAJOR.MINOR.PATCH". 4.3.0 and bare "4" are also tolerated.
- compatibility really filters: query=godot&type=0 -> 943 hits, +compatibility=4.3 -> 427, 4.0 -> 226.
- SearchResults.count is a JSON string ("427"); ReleaseData.size is really a float (0.010494) though the OpenAPI spec types it boolean; max_godot_version is nullable ("4.0" / null).
- Download URLs are presigned and expire.
Validation: cargo test (14 suites, 2 new unit tests + 3 new wiremock round-trips) green; cargo clippy --all-targets clean; cargo fmt --check clean. The get_releases wiremock mock deliberately serialises the number-typed `size` field so the round-trip proves the client ignores the mis-typed field.

Review fixes applied (2026-09-18): shared de helpers now live in src/utils/de.rs; StoreRelease drops redundant serde renames; the three client fns take Option<&GodotVersion> (client derives the MAJOR.MINOR compatibility/godot_version string via GodotVersion::major_minor(), patch and suffixes never sent for store); ggg search --godot-version is parsed through GodotVersion::from_str (rejects e.g. 4.3-stable with a clear error). Search/add tests still assert 'on Godot 4.2./4.3.' and godot_version=4.2/4.3. The get_releases wiremock mock keeps the number-typed  field (via with_size) on purpose: upstream OpenAPI types it boolean, so serializing it proves the client round-trips while ignoring the mis-typed field. Validation: cargo test (all suites incl. 6 search.rs + 6 wiremock.rs round-trips) green, cargo clippy --all-targets clean, cargo fmt --check clean.

Correction: the unbackticked 'number-typed field' phrase refers to ReleaseData.size - the wiremock mock deliberately serialises it (upstream OpenAPI mis-types it as boolean) to prove the client round-trips while ignoring it.

Follow-up review change (2026-09-18): the special-case  field was removed from the StoreRelease wiremock mock -  and its two call sites in the store_get_releases test are gone, and the 'ReleaseData.size' bullet was dropped from the asset_store.rs module doc. The client ignores many unknown fields; size gets no special treatment. Validation re-run: cargo test (all suites), cargo clippy --all-targets clean, cargo fmt --check clean.

Clean note (no backticks): the StoreRelease wiremock mock no longer has a size field or a with_size builder - both call sites in the store_get_releases test were removed along with the ReleaseData.size bullet in the asset_store.rs module doc. size is treated exactly like every other field the client ignores.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
New godot/asset_store.rs client for the Godot Asset Store API (https://store.godotengine.org/api/v1), modeled on asset_lib.rs.

Exposes search(query, godot_version) -> (Vec<StoreAsset>, u32), get_asset(publisher, slug) -> StoreAsset, and get_releases(publisher, slug, godot_version, stable_only) -> Vec<StoreRelease>, with the base URL overridable via GGG_ASSET_STORE_API_URL (new ASSET_STORE_API_URL_ENV_VAR in envvars.rs, registered as pub mod asset_store, documented in docs/reference/environment.md).

Review revision: the Godot version filters now take Option<&GodotVersion> - the clients derive the MAJOR.MINOR string themselves via the new GodotVersion::major_minor() helper (patch and suffixed forms like 4.3-stable are never sent; ggg search --godot-version is parsed through GodotVersion::from_str). The shared string/empty-hash deserializers moved from asset_lib.rs into src/utils/de.rs (pub mod de) and are imported by both clients; redundant #[serde(rename=...)] attributes on StoreRelease min/max_godot_version were dropped; all 'deserialise' spellings were normalized to 'deserialize'.

Only needed fields are deserialized: StoreAsset { slug, publisher{name,slug}, name, license_type, store_url } shared by search and get_asset, and StoreRelease { id, version, stable, download_url, min_godot_version, max_godot_version: Option<String> }. Unknown JSON is ignored - nothing about the mis-typed upstream ReleaseData.size is special-cased in the mock or the docs. SearchResults.count is parsed as a string (de_string_u32), matching the live API, and max_godot_version is nullable.

compatibility format confirmed against the live API: MAJOR.MINOR (4.3) is accepted and really filters (query=godot&type=0: 943 hits, +compatibility=4.3: 427); 4.3-stable returns 422. get_releases forwards stable_only/compatibility to the server only (no client-side filtering); search is first-page-only returning the total via the string count field. Both verified by asserting outgoing query params via wiremock received_requests.

Verified with cargo test (all suites green: 2 asset_store URL unit tests, 3 new store wiremock round-trips, 6 search.rs, plus existing suites), cargo clippy --all-targets clean, cargo fmt --check clean.
<!-- SECTION:FINAL_SUMMARY:END -->
