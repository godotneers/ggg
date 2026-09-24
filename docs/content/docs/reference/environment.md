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

## `GGG_GODOT_DATA_DIR`

Overrides the Godot editor data directory used when installing export templates. When set, ggg installs templates under this path instead of the platform default. On Linux this is normally `~/.local/share/godot/`; on macOS `~/Library/Application Support/Godot/`; on Windows `%APPDATA%\Godot\`.

```bash
GGG_GODOT_DATA_DIR=/tmp/godot-data ggg sync
```

Useful for integration tests and CI environments where writing to the real Godot data directory is undesirable.

## Endpoint overrides

Each of the hardcoded remote endpoints ggg talks to (Godot versions manifest, engine builds API, asset library API, asset store API, export template downloads) can be overridden with an environment variable. This is primarily intended for testing and for mirroring the endpoints, letting you point ggg at a local or proxied server without code changes.

When unset, the built-in default URL is used.

| Variable                       | Overrides                                                       | Default                                                                                 |
|--------------------------------|-----------------------------------------------------------------|-----------------------------------------------------------------------------------------|
| `GGG_GODOT_MANIFEST_URL`       | Godot versions manifest (`GET` the whole YAML)                  | `https://raw.githubusercontent.com/godotengine/godot-website/master/_data/versions.yml` |
| `GGG_GODOT_BUILDS_API_URL`     | Godot builds GitHub releases API base (release tag is appended) | `https://api.github.com/repos/godotengine/godot-builds/releases/tags`                   |
| `GGG_ASSET_LIB_API_URL`        | Godot Asset Library API base                                    | `https://godotengine.org/asset-library/api`                                             |
| `GGG_ASSET_STORE_API_URL`      | Godot Asset Store API base (v1)                                 | `https://store.godotengine.org/api/v1`                                                  |
| `GGG_GODOT_DOWNLOADS_BASE_URL` | Godot downloads base host used for export templates             | `https://downloads.godotengine.org`                                                     |

```bash
GGG_GODOT_MANIFEST_URL=http://localhost:8080/versions.yml ggg sync
```

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
