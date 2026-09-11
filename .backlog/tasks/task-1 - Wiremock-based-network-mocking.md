---
id: TASK-1
title: Wiremock-based network mocking
status: To Do
assignee: []
created_date: '2026-09-08 05:26'
updated_date: '2026-09-11 04:45'
labels: []
dependencies: []
type: feature
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Most of the CLI's integration tests can't run hermetic: sync, search, update, add asset, and engine downloads hit the real network (asset library API, versions manifest, git remotes, GitHub releases). This initiative introduces wiremock + local file:// git repos so the test suite needs no real network.

Decisions already made:
- Git deps in tests use local bare repos via file:// URLs (gix's blocking-network-client provides the file:// transport).
- Zip responses (godot engine, asset deps, export templates) are generated in the test harness with the zip crate, not committed as fixtures.
- Dev-dependencies get wiremock, tokio, serde_json and mirrors of gix, zip, sha2.

Out of scope:
- Interactive add asset <query> disambiguation (untestable in practice)
- Optional deps asset-lib listing
- Interactive init prompts (covered by the non-interactive init mode in TASK-1.14)
- edit/run launches (tracked in TASK-1.15)

TASK-1.5 through TASK-1.10 are independent once TASK-1.3 and TASK-1.4 land.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Full cargo test / clippy / fmt --check pass with no real network
- [ ] #2 No test in the integration suite requires real network access; the interactive add asset disambiguation is the only untracked coverage gap
<!-- AC:END -->
