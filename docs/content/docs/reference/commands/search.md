+++
title = "ggg search"
weight = 10
+++

```
ggg search <query> [--godot-version <version>]
```

Searches the [Godot Asset Library](https://godotengine.org/asset-library/) and prints a table of matching assets. Use this to browse available addons before adding one.

If a `ggg.toml` is present in the current directory, the Godot version declared in `[project]` is used to filter results to compatible assets. Pass `--godot-version` to override this, or run the command outside a project directory to see all results.

## Flags

**`--godot-version`:** filter results to assets compatible with this Godot version (e.g. `4.3`). Overrides the version from `ggg.toml`.

## Example output

```
$ ggg search gut
Searching for "gut" (Godot 4.3) ...

  id   name                              version  author
  ---  --------------------------------  -------  ------
  54   GUT - Godot Unit Testing          9.3.0    bitwes
  231  GUT Godot Unit Test Utilities     1.0.0    example

Use `ggg add asset --id <N>` to add a result.
```

## See also

- [`ggg add asset`](@/docs/reference/commands/add.md): add an asset library dependency (includes an interactive search)
- [`ggg update`](@/docs/reference/commands/update.md): check for newer versions of installed asset library dependencies
