+++
title = "Environment variables"
weight = 4
+++

ggg reads the following environment variables at runtime.

## `GGG_CACHE_DIR`

Overrides the default location of the shared cache directory. When set, all cached Godot binaries and dependency archives are stored under this path instead of the platform default.

| Platform | Default (when unset) |
|----------|----------------------|
| Linux    | `~/.local/share/ggg/` |
| macOS    | `~/Library/Application Support/ggg/` |
| Windows  | `%APPDATA%\ggg\` |

```bash
GGG_CACHE_DIR=/mnt/fast-ssd/ggg-cache ggg sync
```

Useful for CI environments where you want to place the cache on a specific volume, or for developers who prefer to keep all tool caches in a central location.

See the [cache reference](@/docs/reference/cache.md) for details about the directory structure.

## `NO_COLOR`

When set to any value, `ggg diff` suppresses coloured output and emits plain unified diff text instead. Set automatically by most CI environments.

```bash
NO_COLOR=1 ggg diff
```

## Proxy variables

ggg uses `reqwest` for all HTTP requests (Godot downloads, asset library API calls, archive dependency downloads). `reqwest` automatically honours the standard proxy environment variables:

| Variable | Purpose |
|----------|---------|
| `HTTPS_PROXY` | Proxy for HTTPS requests |
| `HTTP_PROXY` | Proxy for HTTP requests |
| `NO_PROXY` | Comma-separated list of hosts to connect to directly, bypassing the proxy |

Lowercase variants (`https_proxy`, `http_proxy`, `no_proxy`) are also accepted.

```bash
HTTPS_PROXY=http://proxy.example.com:8080 ggg sync
```
