---
id: TASK-1.15
title: 'Spike: test edit/run arg forwarding via a mock Godot binary'
status: To Do
assignee: []
created_date: '2026-09-08 05:49'
labels: []
dependencies:
  - TASK-1.11
parent_task_id: TASK-1
type: spike
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
edit and run success paths spawn the real Godot engine binary, so their arg-forwarding behavior cannot be asserted hermetically. The plan is to place a fake engine executable where the engine is expected that records argv and exits (the mock-Godot helper).

This is a spike: validate the approach before committing to full test suites. Open questions to resolve:
- Where exactly does the harness put the mock binary so edit/run pick it up (engine cache layout, env override)?
- Can the mocked engine-download chain (TASK-1.11) install the fake engine, or is direct seeding needed?
- Windows specifics: .exe vs script, Process spawning, arg quoting.

Deliverable is a validated approach plus a working mock engine helper; the two full test suites may follow as a separate task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Mock engine executable is installed/reachable where edit/run expect the engine
- [ ] #2 Mock records argv and exits successfully
- [ ] #3 edit forwards trailing args verbatim and prepends --editor .
- [ ] #4 run forwards trailing args verbatim and passes the project dir
- [ ] #5 --with-export-templates triggers template install before launch
- [ ] #6 Approach validated on Windows or a Windows-specific blocker documented
<!-- AC:END -->
