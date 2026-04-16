+++
title = "ggg update"
weight = 11
+++

```
ggg update [<name>] [--dry-run]
```

Checks whether a newer version of a Godot Asset Library dependency is available. If one is found, the lock entry is dropped so that the next [`ggg sync`](@/docs/reference/commands/sync.md) fetches and installs it.

Omit `<name>` to check all asset library dependencies at once.

This command only applies to dependencies added via [`ggg add asset`](@/docs/reference/commands/add.md). For git and archive dependencies, update by editing `rev` or `url` in `ggg.toml` and running `ggg sync`.

## How it works

`ggg update` compares the version number stored in `ggg.lock` against the current version reported by the asset library API. If the API version is higher, the lock entry is removed. On the next `ggg sync`, the dependency is treated as unresolved: the new version is fetched, cached, and installed.

If a dependency has no lock entry (i.e. it was added but `ggg sync` has never been run), a message is printed and it is skipped.

## Flags

**`--dry-run`:** report available updates without modifying `ggg.lock`. Useful for checking whether updates exist before deciding to apply them.

## Example output

```
$ ggg update
gut: version 9 -> v9.3.1 - run `ggg sync` to install.
phantom-camera: up to date (v0.8).
```

```
$ ggg update gut --dry-run
gut: update available: version 9 -> v9.3.1.
```

## See also

- [`ggg sync`](@/docs/reference/commands/sync.md): install updates after running `ggg update`
- [`ggg add asset`](@/docs/reference/commands/add.md): add an asset library dependency
- [`ggg search`](@/docs/reference/commands/search.md): browse the asset library
