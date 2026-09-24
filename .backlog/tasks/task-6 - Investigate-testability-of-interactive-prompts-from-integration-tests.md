---
id: TASK-6
title: Investigate testability of interactive prompts from integration tests
status: To Do
assignee: []
created_date: '2026-09-24 06:45'
labels: []
dependencies: []
references:
  - TASK-5.9
priority: low
type: spike
ordinal: 32000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Deferred from TASK-5 (superseding TASK-5.9); carried over verbatim.

The reworked `ggg add` keeps interactive dialoguer flows: the 2-5 hit asset-search picker (store-only, library-only, and combined) and the name/spec Input prompts. Integration tests in tests/add.rs deliberately avoid these paths because driving dialoguer from assert_cmd is fragile, so the pickers are effectively untested.

This spike should find a robust way to exercise interactive prompts in integration tests (e.g. dialoguer/assert_cmd stdin wiring, a ptty wrapper, or restructuring the prompt layer so selections are testable without a terminal) and report a concrete recommendation plus a working proof-of-concept test. No production behaviour changes; TASK-5.6 already ships with the picker covered only by unit tests around the surrounding logic.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A working integration test drives a dialoguer Select (the ggg add search picker) to completion
- [ ] #2 A working integration test drives a dialoguer Input prompt to completion
- [ ] #3 A written recommendation covers the trade-offs of each tested approach and names the one ggg should standardise on
- [ ] #4 Proof-of-concept tests run in CI-style headless conditions (no TTY required)
<!-- AC:END -->
