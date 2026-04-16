+++
title = "ggg sync"
weight = 4
+++

```
ggg sync [--dry-run] [--force]
```

Downloads the Godot version declared in `ggg.toml` (if not already cached), resolves and installs all dependencies, and removes files left behind by dependencies that have been deleted or remapped. Run this after any change to `ggg.toml`.

## When to run

- After [`ggg init`](@/docs/reference/commands/init.md), to download Godot.
- After [`ggg add`](@/docs/reference/commands/add.md) or [`ggg remove`](@/docs/reference/commands/remove.md), to apply the change to the project.
- After cloning a repository that already has a `ggg.toml`, to set up the full project environment.
- After updating a dependency's `rev` or `url` in `ggg.toml`.

## What it does

`ggg sync` works in two phases. It first computes everything that would change without touching any files, then carries out all changes at once. If anything causes a conflict, the whole sync is aborted before any files are written.

**Godot:** downloads and caches the Godot binary declared in `[project]` if it is not already present. Does nothing if the version is already cached.

**Dependencies:** for each dependency in `ggg.toml`:
- Resolves the revision to a pinned commit SHA (git deps), verifies the archive hash (archive deps), or fetches the current download URL from the Godot Asset Library (asset deps). The lock file is used to skip this step when nothing has changed.
- Downloads and caches the dependency if it is not already cached.
- Installs the files into the project, applying `strip_components` and `map` as configured. Asset library dependencies default to `strip_components = 1`.

**Cleanup:** removes files that were installed by a dependency that has since been removed from `ggg.toml`, or that no longer appear in a dependency's `map`.

**Lock file and state:** writes `ggg.lock` with the resolved identities of all dependencies, and updates `.ggg.state` with the list of installed files and their content hashes. Both are written atomically at the end of a successful sync.

## Example output

```
$ ggg sync
  gut (v9.3.0 -> fbfabd5052e9): installed 257 files (257 total)
  phantom-camera (v0.8 -> a1b2c3d4ef56): up to date (312 files)
```

## Conflict detection

`ggg sync` tracks every file it installs in `.ggg.state`. On the next sync it checks whether each file is still in the state it left it in. A conflict is raised when:

- A file that `ggg` would overwrite has been modified since it was installed.
- A file that `ggg` would write already exists but was not installed by `ggg` at all.

When conflicts are detected, `ggg sync` prints the affected files and exits without writing anything:

```
Conflicts detected - sync would fail without --force:

  gut:
    addons/gut/plugin.cfg  (modified since last install)
    addons/gut/gut_utils.gd  (not under ggg's control)
```

Use [`ggg diff`](@/docs/reference/commands/diff.md) to review what has changed before deciding how to proceed.

## Flags

**`--dry-run`:** computes and prints the full plan without writing any files. Useful for previewing what a sync would change. Exits cleanly even if there are conflicts.

**`--force`:** overwrites all conflicting files without prompting, discarding any local changes. Use this when you are sure the conflicts do not matter.

## Automatically overwriting engine-managed files

The Godot editor silently rewrites `.import` metadata and `.uid` files every time a project is opened. These rewrites show up as conflicts on the next sync even though you did not change them yourself.

To suppress this, add a `[sync]` table to `ggg.toml`:

```toml
[sync]
force_overwrite = ["**/*.import", "**/*.uid"]
```

Any file whose path matches one of these glob patterns is unconditionally overwritten, bypassing conflict detection. See the [configuration reference](@/docs/reference/configuration.md) for details.

## Files written

| File | Purpose | Commit to git? |
|------|---------|----------------|
| `ggg.lock` | Pinned dependency versions | Yes |
| `.ggg.state` | Installed file tracking (per-checkout) | No (added to `.gitignore` automatically) |

## See also

- [`ggg diff`](@/docs/reference/commands/diff.md): review local changes to installed files
- [`ggg add`](@/docs/reference/commands/add.md): add a dependency
- [`ggg remove`](@/docs/reference/commands/remove.md): remove a dependency
- [Configuration reference](@/docs/reference/configuration.md): `[sync]`, `force_overwrite`, `strip_components`, `map`
