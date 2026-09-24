+++
title = "ggg search"
weight = 10
+++

```
ggg search <query> [--source <source>] [--godot-version <version>]
```

Searches for addons and prints a table of matches. Use this to browse available addons before adding one.

By default `ggg search` queries the [Godot Asset Store](https://store.godotengine.org/). If you want to search in the legacy Godot [Asset Library](https://godotengine.org/asset-library/) use  `--source asset-library`.

If a `ggg.toml` is present in the current directory, the Godot version declared in `[project]` is sent to the source for compatibility filtering (as `compatibility` for the Asset Store, `godot_version` for the Asset Library). Pass `--godot-version` to override this, or run the command outside a project directory to skip the filter.

## Flags

**`--source`:** the source to search: `asset-store` (default) or `asset-library`. Any other value is rejected.

**`--godot-version`:** override the Godot version used for compatibility filtering (e.g. `4.3`). Overrides the version from `ggg.toml`.

## Example output

### Asset Store (default source)

The first column shows the `publisher_slug/asset_slug` reference you would pass to `ggg add asset-store`, so you can copy it straight from the output:

```
$ ggg search gut
  publisher/slug    Name                          License
  ----------------  ----------------------------  -------
  bitwes/gut        GUT - Godot Unit Testing      MIT
  example/gut-addon GUT Godot Unit Test Utilities MIT

Found 2 results on Godot 4.3.

Use `ggg add asset-store <publisher>/<slug>` to add a specific asset.
```

### Asset Library

Run the same command with `--source asset-library` to search the legacy library instead:

```
$ ggg search terrain --source asset-library
  ID     Title              Author        License
  -----  -----------------  ------------  -------
  1586   Terrain Generator   Lox           MIT
  42     Toolbox             Some Author   CC0

Found 2 results on Godot 4.3.

Use `ggg add asset-library --id <N>` to add a specific asset.
```

The Asset Library table shows the numeric **ID** you would pass to `ggg add asset-library --id <N>`.

## See also

- [`ggg add`](@/docs/reference/commands/add.md): add a dependency, including Asset Store and Asset Library assets
- [`ggg update`](@/docs/reference/commands/update.md): check for newer versions of installed asset library dependencies