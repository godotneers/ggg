+++
title = "ggg deps"
weight = 9
+++

```
ggg deps
```

Lists all dependencies declared in `ggg.toml` as a table. Each row shows the dependency name, its type (git, archive, or asset), and the version or source identifier.

## Example output

```
$ ggg deps
name             type     version / source
---------------  -------  -------------------------
gut              asset    id=54
phantom-camera   git      v0.8
debug-draw       archive  https://example.com/d.zip
```

For git dependencies the `rev` field from `ggg.toml` is shown. For archive dependencies the URL is shown. For asset library dependencies the asset ID is shown.

`ggg deps` reads only `ggg.toml`; it does not consult the lock file or the network.

## See also

- [`ggg add`](@/docs/reference/commands/add.md): add a dependency
- [`ggg remove`](@/docs/reference/commands/remove.md): remove a dependency
- [`ggg sync`](@/docs/reference/commands/sync.md): install or update dependencies
