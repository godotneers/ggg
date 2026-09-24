+++
title = "ggg update"
weight = 11
+++

```
ggg update [<name>] [--dry-run]
```

Checks whether a newer version of a Godot Asset Library or Asset Store dependency is available. If one is found, the lock entry is dropped so that the next [`ggg sync`](@/docs/reference/commands/sync.md) fetches and installs it. For Asset Store dependencies, the pinned version in `ggg.toml` is also bumped to the newest compatible release.

Omit `<name>` to check all eligible dependencies at once.

This command only applies to dependencies added via [`ggg add asset-library`](@/docs/reference/commands/add.md) or [`ggg add asset-store`](@/docs/reference/commands/add.md). For git and archive dependencies, update by editing `rev` or `url` in `ggg.toml` and running `ggg sync`.

## How it works

### Asset Library

`ggg update` compares the version number stored in `ggg.lock` against the current version reported by the asset library API. If the API version is higher, the lock entry is removed. On the next `ggg sync`, the dependency is treated as unresolved: the new version is fetched, cached, and installed.

### Asset Store

Because an Asset Store dependency always pins a version (`publisher/slug:version` in `ggg.toml`), `ggg update` compares that pinned version - via the version stored in `ggg.lock` - against the newest stable release compatible with the project's Godot version. "Newest" is decided by semantic version comparison; the release id only breaks ties between releases that share the same version string. This means a later-uploaded patch of an older series never outranks a genuinely higher version, and the command never downgrades a dependency.

If a newer release exists, `ggg update` bumps the pinned version in `ggg.toml` and drops the lock entry, so the next `ggg sync` downloads and installs the new release.

### Both

`ggg update` never guesses. Before querying the asset library or store it verifies that `ggg.lock` and `ggg.toml` agree, and aborts (without touching the network or any file) when they do not:

- a dependency with no lock entry (added but `ggg sync` has never been run),
- a lock entry with no matching entry in `ggg.toml` (stale entry),
- a dependency whose source kind changed in `ggg.toml` (e.g. git to archive) since the lock entry was written,
- a lock-key field edited in `ggg.toml` (the asset library id, or the pinned store release version),

all fail the command with a pointer to [`ggg sync`](@/docs/reference/commands/sync.md), which reconciles the two files. `ggg update <name>` validates only the named dependency; without a name, the whole project must be in sync before anything is checked.

## Flags

**`--dry-run`:** report available updates without modifying `ggg.lock` (or `ggg.toml`). Useful for checking whether updates exist before deciding to apply them.

## Example output

```
$ ggg update
gut: version 9 -> v9.3.1 - run `ggg sync` to install.
phantom-camera: up to date (v0.8).
souleat/godot-xoshiro256-plus-plus: version 1.1.0 -> v1.2.0 - run `ggg sync` to install.
```

```
$ ggg update gut --dry-run
gut: update available: version 9 -> v9.3.1.
```

```
$ ggg update soul/godot-xoshiro256-plus-plus --dry-run
godot-xoshiro256-plus-plus: update available: version 1.1.0 -> v1.2.0 (release id 42).
```

## See also

- [`ggg sync`](@/docs/reference/commands/sync.md): install updates after running `ggg update`
- [`ggg add asset-library`](@/docs/reference/commands/add.md): add an asset library dependency
- [`ggg add asset-store`](@/docs/reference/commands/add.md): add an asset store dependency
- [`ggg search`](@/docs/reference/commands/search.md): browse the asset library