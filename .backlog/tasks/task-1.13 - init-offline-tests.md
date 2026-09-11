---
id: TASK-1.13
title: init offline tests
status: To Do
assignee: []
created_date: '2026-09-08 05:28'
updated_date: '2026-09-08 05:32'
labels: []
dependencies: []
parent_task_id: TASK-1
type: task
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The parts of init reachable without interactive prompts or the network:
- Fails with an error when ggg.toml already exists
- Unit: create_project_godot writes config_version 4 (3.x) / 5 (4.x) and config/features for Godot 4+
- Unit: ensure_gitignore_entry creates the file when missing
- Unit: ensure_gitignore_entry appends when the file lacks a trailing newline
- Unit: ensure_gitignore_entry no-ops when the entry is already present
The interactive network-backed init cases remain out of scope for this effort.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Fails with an error when ggg.toml already exists
- [ ] #2 Unit: create_project_godot writes config_version 4 (3.x) / 5 (4.x)
- [ ] #3 Unit: create_project_godot writes config/features for Godot 4+
- [ ] #4 Unit: ensure_gitignore_entry creates the file when missing
- [ ] #5 Unit: ensure_gitignore_entry appends when no trailing newline
- [ ] #6 Unit: ensure_gitignore_entry no-ops when already present
<!-- AC:END -->
