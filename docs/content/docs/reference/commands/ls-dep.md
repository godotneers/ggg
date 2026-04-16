+++
title = "ggg ls-dep"
weight = 7
+++

```
ggg ls-dep <name> [--all]
```

Lists the raw contents of a dependency's source tree as fetched from the remote, before `strip_components` or `map` are applied. Use this to determine the right values for those fields after adding a new dependency.

`<name>` must match the `name` field of a dependency already declared in `ggg.toml`.

If the dependency is already in the local cache (e.g. from a previous `ggg sync`), the cached copy is used without re-fetching. Otherwise the dependency is fetched and cached, and `ggg.lock` is updated as a side effect.

## Flags

| Flag | Description |
|------|-------------|
| `--all` | Show every file path individually. By default, directories are collapsed to a single line with a file count. |

> **Note:** This command is not yet implemented.
