---
id: TASK-4
title: Allow overriding the Godot executable
status: Done
assignee: []
created_date: '2026-09-14 05:20'
updated_date: '2026-09-24 17:39'
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
1. --godot <path> CLI flag (uv --python analog), respected by ggg run, ggg edit, AND ggg sync (sync added after review for parity: sync never launches Godot, so its flag only means "do not manage/download the engine", same as the env var, with the guardrail still applying).
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
- [x] #1 MECHANISM 1 - A --godot <path> CLI flag (uv --python analog) selects the Godot executable and is respected by ggg run, ggg edit, and ggg sync
- [x] #2 MECHANISM 2 - GGG_GODOT_EXECUTABLE env var selects the Godot executable, registered as a GGG_* constant in src/envvars.rs following the existing pattern
- [x] #3 MECHANISM 3 - Do not manage: when an override resolves the executable, ggg performs no managed download and uses the user-supplied executable as-is
- [x] #4 PRECEDENCE - Override order is --godot > GGG_GODOT_EXECUTABLE > managed default; with no override set, the existing managed download of the release pinned in ggg.toml [project] is preserved unchanged
- [x] #5 PORTABILITY - ggg.toml is deliberately NOT extended with an executable-path option (portability boundary, peer-verified); committed config stays portable and overrides exist only in the flag and the env var so they are never committed
- [x] #6 GUARDRAIL - An invalid/nonexistent executable path produces a clear error rather than a panic or an unexpected managed download
- [x] #7 DOCS - Documentation updated (docs/, CHANGES.md) and tests cover each override mechanism plus the precedence ordering
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. src/envvars.rs: add GODOT_EXECUTABLE_ENV_VAR = "GGG_GODOT_EXECUTABLE" following the GGG_* + doc-comment pattern.
2. src/godot/engine.rs: add env_executable() (empty value = unset), validate_executable() guardrail (clear error naming --godot / $GGG_GODOT_EXECUTABLE, never panics/never downloads), and resolve() composing cli-override-or-env -> validate else ensure(). Keep ensure()/launch() untouched. The precedence helper was inlined (review feedback: override_path existed only for unit tests; end-to-end precedence is covered by integration tests).
3. Commands: run/edit/sync gain `godot: Option<String>` as the --godot flag (sync added for flag/env parity per review; sync never launches Godot, so the flag simply opts out of the managed download with the guardrail still applied). main.rs: `#[arg(long)] godot` on Run, Edit, and Sync variants, threaded into dispatch.
4. Tests: unit tests in engine.rs for env parsing (empty = unset) and validation messages. tests/override.rs integration suite; reference binary is a noop executable (`fn main() {}`) compiled with rustc at test time via tests/common/mod.rs noop_executable(), written into CARGO_TARGET_TMPDIR so cargo clean removes it and compiled without --edition (review feedback; the trivial source needs no edition). Cover: guardrail (missing path -> clear error, empty cache, no managed download) for flag and env var, directory path rejected, precedence both ways, positive launch of run/edit via flag and env var, sync honoring flag and env var without downloading.
5. Docs + changelog: run.md/edit.md/sync.md --godot flag; sync.md engine paragraph; environment.md GGG_GODOT_EXECUTABLE with export-then-call example (review feedback); CHANGES.md [Unreleased] Added.
6. Verify: cargo test, cargo clippy, cargo fmt --check (full suite needs -j 4 on this machine: parallel rustc link of heavy gix test binaries crashes with STATUS_STACK_BUFFER_OVERRUN).

Design decisions (confirmed with user): flag respected by run, edit, AND sync (sync parity added on review); env var honored by run/edit/sync; guardrail = existence + regular-file check only (no binary probe / --version); positive E2E launch test uses a noop executable compiled with rustc at test time; ggg.toml NOT extended (portability boundary).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verification results: full cargo test suite green (313 lib tests + 129 integration/doc tests incl. 11 new override E2E tests and 10 new engine unit tests), cargo clippy --all-targets clean, cargo fmt --check clean. On this Windows machine the full suite must run with -j 4; the default job count makes parallel rustc links of heavy gix test binaries crash (0xC0000409 STATUS_STACK_BUFFER_OVERRUN), unrelated to code changes. Docs updated: run.md/edit.md --godot flag, sync.md env override note, environment.md GGG_GODOT_EXECUTABLE section, CHANGES.md [Unreleased] Added.

Review round: (1) ggg sync gained a --godot flag for parity with run/edit (user-approved; it only opts out of the managed engine, guardrail applies). (2) environment.md example now uses export GGG_GODOT_EXECUTABLE=... then a separate ggg sync call, and the wording no longer claims the declared version is "ignored for execution" (sync never executes). (3) override_path() inlined into resolve() and its 3 unit tests removed (review: existed only for tests; precedence covered by godot_flag_wins_over_invalid_env_var + godot_flag_overrides_valid_env_var). (4) noop_executable() no longer passes --edition and writes into CARGO_TARGET_TMPDIR so cargo clean removes the artifact. Verification: cargo test -j 4 all green (310 lib + 13 override + full integration/doc suites), cargo clippy --all-targets clean, cargo fmt --check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented the Godot executable override: --godot <path> flag (ggg run, ggg edit, and ggg sync) and GGG_GODOT_EXECUTABLE env var (run/edit/sync), resolving via engine::resolve() which validates the override (existing regular file, error names the source, never panics/never downloads) and falls back to the untouched managed ensure() default; precedence --godot > env var > managed default; ggg.toml intentionally not extended. Review round applied: sync parity flag, precedence inlined (override_path + its unit tests removed), noop test binary built without --edition under CARGO_TARGET_TMPDIR, env doc example made explicit (export then ggg sync). Verified with engine/env unit tests plus 13 integration tests in tests/override.rs (guardrails for flag and env var, precedence both directions, positive launches via a rustc-compiled noop executable, sync honoring flag and env var, no managed download); full cargo test suite, cargo clippy --all-targets, and cargo fmt --check all clean.
<!-- SECTION:FINAL_SUMMARY:END -->
