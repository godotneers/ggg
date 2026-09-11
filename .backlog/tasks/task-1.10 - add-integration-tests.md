---
id: TASK-1.10
title: add integration tests
status: Done
assignee: []
created_date: '2026-09-08 05:27'
updated_date: '2026-09-11 05:11'
labels: []
dependencies:
  - TASK-1.3
  - TASK-1.4
parent_task_id: TASK-1
type: task
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
tests/add.rs. Archive offline cases: writes dep, rejects a non-archive URL ext, bare routing of add <url>.zip, duplicate name rejected, no-args error, no-ggg.toml error. add git <file://url>@rev --name -y through the local bare repo. add asset --id N --name -y via wiremock. Plus a unit test for infer_name_from_asset in src/commands/add.rs.

Integration tests cover only the non-interactive add paths: add git / add archive / add asset via --id N -y, all avoid prompts. The interactive add asset <query> disambiguation (0 / 1 / 2-5 / 6+ results) cannot be tested reliably - driving dialoguer from a test harness is fragile - and stays permanently out of scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 add archive <url> --name X writes the dep with no network
- [x] #2 add archive rejects a non-.zip/.tar.gz/.tgz URL
- [x] #3 Bare add <url>.zip routes to the archive path
- [x] #4 Fails with 'no ggg.toml found'
- [x] #5 Duplicate name rejected
- [x] #6 add with no arguments fails
- [x] #7 add git <file://url>@<rev> --name -y resolves and writes the dep
- [x] #8 add asset --id <N> --name -y fetches and writes the dep via wiremock
- [x] #9 Unit test covers infer_name_from_asset splitting and normalising
- [x] #10 Interactive add asset <query> disambiguation is explicitly out of scope; only the non-interactive --id/-y path is tested
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented all tests. All 226 unit tests + 8 new integration tests pass. cargo clippy and cargo fmt --check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
- Added 6 unit tests for infer_name_from_asset in src/commands/add.rs.
- Added 8 integration tests in tests/add.rs covering: archive add/rejection/bare routing, missing ggg.toml, duplicate name, no-args, git file:// resolve, and asset --id via wiremock.
- Interactive disambiguation documented as permanently out of scope.
- Verified: cargo test (226 unit + 8 integration), cargo clippy, cargo fmt --check all pass.
<!-- SECTION:FINAL_SUMMARY:END -->
