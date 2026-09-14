---
id: TASK-4
title: Allow overriding the Godot executable
status: To Do
assignee: []
created_date: '2026-09-14 05:20'
updated_date: '2026-09-14 05:32'
labels: []
dependencies: []
references:
  - 'https://github.com/godotneers/ggg/issues/2'
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
GitHub issue #2 (https://github.com/godotneers/ggg/issues/2): users sometimes need to supply their own Godot binary instead of letting ggg download and manage it, e.g. a locally patched core build or a distribution-managed install. Today ggg run, ggg edit, and ggg sync all resolve the executable through engine::ensure using the release pinned in ggg.toml and the shared GodotCache (src/godot/engine.rs + src/godot/cache.rs), which downloads that release whenever it is missing. There is no way to point ggg at an existing binary or to opt out of the managed download.

The override mechanisms mirror uv --python / UV_PYTHON. There are exactly THREE user-facing override mechanisms; everything else in this task is the precedence rule, the unchanged default, and guardrails around them.

THREE OVERRIDE MECHANISMS
1. --godot <path> CLI flag (uv --python analog), respected by ggg run and ggg edit.
2. GGG_GODOT_EXECUTABLE environment variable (uv UV_PYTHON analog).
3. Do not manage: supplying any override tells ggg NOT to manage the binary at all - no managed download, the user-supplied executable is used as-is.

PRECEDENCE RULE
--godot flag > GGG_GODOT_EXECUTABLE env var > managed default. With no override set, the existing managed behavior is preserved unchanged: engine::ensure downloads the release pinned in ggg.toml [project] into the shared cache. This ordering was peer-verified against the uv docs during planning.

PORTABILITY BOUNDARY (design decision, peer-verified)
ggg.toml is deliberately NOT extended with an executable-path option. A committed absolute path would not port across machines (ggg.toml is meant to be committed). Machine-specific overrides therefore live only in the CLI flag and the env var - neither of which is ever committed - and the portable managed default remains in ggg.toml.

Guardrails: an invalid/nonexistent executable path must produce a clear error. Follow the existing GGG_* pattern in src/envvars.rs (new GGG_GODOT_EXECUTABLE constant) and the from_env() env-var lookup style already used by GodotCache.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 MECHANISM 1 - A --godot <path> CLI flag (uv --python analog) selects the Godot executable and is respected by ggg run and ggg edit
- [ ] #2 MECHANISM 2 - GGG_GODOT_EXECUTABLE env var selects the Godot executable, registered as a GGG_* constant in src/envvars.rs following the existing pattern
- [ ] #3 MECHANISM 3 - Do not manage: when an override resolves the executable, ggg performs no managed download and uses the user-supplied executable as-is
- [ ] #4 PRECEDENCE - Override order is --godot > GGG_GODOT_EXECUTABLE > managed default; with no override set, the existing managed download of the release pinned in ggg.toml [project] is preserved unchanged
- [ ] #5 PORTABILITY - ggg.toml is deliberately NOT extended with an executable-path option (portability boundary, peer-verified); committed config stays portable and overrides exist only in the flag and the env var so they are never committed
- [ ] #6 GUARDRAIL - An invalid/nonexistent executable path produces a clear error rather than a panic or an unexpected managed download
- [ ] #7 DOCS - Documentation updated (docs/, CHANGES.md) and tests cover each override mechanism plus the precedence ordering
<!-- AC:END -->
