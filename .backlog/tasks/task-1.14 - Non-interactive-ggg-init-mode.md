---
id: TASK-1.14
title: Non-interactive ggg init mode
status: To Do
assignee: []
created_date: '2026-09-08 05:48'
labels: []
dependencies:
  - TASK-1.3
parent_task_id: TASK-1
type: task
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
init currently always prompts interactively: a FuzzySelect picks the Godot version from the network-fetched manifest, and Confirm prompts decide mono and export templates. The whole happy path is therefore untestable hermetically (driving dialoguer from a test harness is fragile).

Add a flag-driven path alongside the interactive one, following the add --yes precedent:
- --release <version>: selects the Godot release non-interactively; requires it to exist in the fetched manifest.
- --mono / --no-mono: override mono; when absent, inherit from project.godot if present, else default to false.
- --with-export-templates: already exists and decides templates.
Supplying --release makes init non-interactive (no prompts); omitting it keeps today's interactive behavior. The full success path becomes testable through the binary with the versions manifest mocked via wiremock.

The interactive init prompts themselves remain as the UX, they are just not covered by tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ggg init --release <version> creates ggg.toml with the chosen release without prompting
- [ ] #2 Creates a minimal project.godot when none is present
- [ ] #3 Reads version + mono flag from an existing project.godot
- [ ] #4 Adds .ggg.state to .gitignore, creating the file if missing
- [ ] #5 Does not duplicate .ggg.state in an existing .gitignore
- [ ] #6 --mono / --no-mono override, inheriting from project.godot or defaulting to false
- [ ] #7 Unknown release in --release errors cleanly
- [ ] #8 Full success path tested through the binary with the manifest mocked via wiremock
- [ ] #9 Interactive behavior unchanged when --release is omitted
<!-- AC:END -->
