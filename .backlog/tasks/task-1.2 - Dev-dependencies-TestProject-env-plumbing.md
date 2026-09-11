---
id: TASK-1.2
title: Dev-dependencies + TestProject env plumbing
status: Done
assignee: []
created_date: '2026-09-08 05:27'
updated_date: '2026-09-11 05:11'
labels: []
dependencies: []
parent_task_id: TASK-1
type: task
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Cargo.toml: add wiremock, tokio (rt-multi-thread), serde_json to [dev-dependencies]; mirror gix, zip, sha2 so fixture helpers can use them.
Extend tests/common/mod.rs: TestProject gains env() plumbing for all GGG_* vars plus a block_on helper for wiremock's async startup.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Cargo.toml dev-dependencies include wiremock, tokio (rt-multi-thread + macros), serde_json, gix, zip, sha2
- [x] #2 TestProject can set any GGG_* env var per project (applied to the spawned command)
- [x] #3 Wiremock async startup runs inside tests via the tokio test runtime (#[tokio::test])
- [x] #4 Existing tests still pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Cargo.toml: add wiremock, serde_json, and tokio (rt-multi-thread + macros) to [dev-dependencies]; mirror gix (0.81.0), zip (2), sha2 (0.10) at main-dep versions so fixture helpers can use them.
2. tests/common/mod.rs: add an envs map + builder env() setter to TestProject; apply vars via .env() inside cmd(); keep cfg-builder pattern.
3. Revise acceptance criterion #3: use #[tokio::test] (macros + rt-multi-thread) instead of a hand-written block_on helper.
4. cargo build/test/clippy/fmt --check; confirm existing tests still pass (criterion #4).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented: Cargo.toml dev-deps (wiremock 0.6, tokio rt-multi-thread+macros, serde_json 1, gix 0.81.0, zip 2, sha2 0.10). tests/common/mod.rs: TestProject.env() builder stores env vars, cmd() applies them via cmd.env(). Used #[tokio::test] (macros feature) in place of a hand-written block_on helper per plan review.

Validation: cargo build OK (wiremock 0.6.5, tokio 1.51.1 resolved); cargo test 216 unit + 6 cli + 3 deps + 5 remove all pass; cargo clippy --all-targets clean; cargo fmt --check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
- Added dev-dependencies: wiremock, tokio (rt-multi-thread + macros), serde_json; mirrored gix/zip/sha2 at main-dep versions.
- Extended TestProject in tests/common/mod.rs with an env() builder whose vars are applied to each spawned command via cmd.env().
- Chose #[tokio::test] over a hand-written block_on helper after plan review; acceptance criterion #3 was revised accordingly.
- Verified: cargo build, cargo test (216 unit + 14 integration passing), cargo clippy --all-targets, cargo fmt --check.
<!-- SECTION:FINAL_SUMMARY:END -->
