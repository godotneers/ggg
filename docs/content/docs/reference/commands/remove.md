+++
title = "ggg remove"
weight = 3
+++

```
ggg remove <name>
```

Removes a dependency from `ggg.toml`. The `<name>` argument is the `name` field of the dependency you want to remove, as it appears in `ggg.toml`.

The command only edits `ggg.toml`. The addon files already installed in your project are left untouched until you run [`ggg sync`](@/docs/reference/commands/sync.md), which will clean them up.

## Example

```
$ ggg remove gut
Removed "gut" from ggg.toml.
Run `ggg sync` to uninstall its files from the project.
```

## Notes

- Fails if no dependency with that name exists in `ggg.toml`.
- Comments and formatting outside the `[[dependency]]` blocks in `ggg.toml` are preserved. Comments inside a dependency block are lost (see [known limitations](@/docs/reference/configuration.md)).
- Run `ggg sync` after removing to delete the installed files and update `ggg.lock`.
