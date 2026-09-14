---
id: TASK-3
title: >-
  Sync conflicts when a git dependency ships ggg's own project files
  (.gitignore, ggg.toml)
status: To Do
assignee: []
created_date: '2026-09-14 04:59'
updated_date: '2026-09-14 05:00'
labels: []
dependencies: []
references:
  - 'https://github.com/godotneers/ggg/issues/3'
type: bug
ordinal: 18000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found during a real-world end-to-end test of TASK-2 against https://github.com/JetBrains/godot-support.git (rev master). That repo ships a root `.gitignore`, which `ggg sync` installs into the project as a managed file. After installation `commands/sync.rs` calls `ensure_gitignore_entry` on the project `.gitignore`, appending `.ggg.state` to that same managed file - but the per-file hashes were already recorded in `.ggg.state` at that point, so the recorded `.gitignore` hash is stale the moment it is written. The next `ggg sync` therefore reports a conflict (`.gitignore (modified since last install)`) and requires `--force`. This is independent of submodules and would affect any git dependency that ships a root `.gitignore`.

Closely related concern to verify: a third-party repo that contains its own `ggg.toml` (an addon managed by ggg itself, or a copy-pasted manifest) could write over the project's `ggg.toml`. Hypothesis: if the destination file already exists before the dependency is pulled in, conflict detection blocks the install unless `--force` is given (ggg never silently overwrites), so the danger is limited to metadata files that ggg itself writes after installation (like `.gitignore` above); this needs verification in the same pass.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Two consecutive `ggg sync` runs succeed without a conflict when a git dependency installs a root `.gitignore` (no spurious `.gitignore (modified since last install)` on the second run).
- [ ] #2 A git dependency whose tree contains a `ggg.toml` does not overwrite the project's own `ggg.toml`, `ggg.lock`, or `.ggg.state`: pre-existing files block installation via conflict detection and require `--force`, and a fresh install does not clobber later metadata writes.
- [ ] #3 `.ggg.state` remains gitignored after sync.
- [ ] #4 Regression test coverage for the second-sync gitignore case and the third-party `ggg.toml` case.
- [ ] #5 `cargo test`, `cargo clippy`, and `cargo fmt --check` all pass.
<!-- AC:END -->
