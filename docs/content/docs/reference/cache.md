+++
title = "Cache"
weight = 3
+++

ggg maintains a shared, cross-project cache of downloaded Godot binaries and dependency archives. Caching means that the second time you install a dependency (whether in the same project or a different one) no download is needed.

The cache is safe to delete at any time. ggg will re-download anything it needs on the next `ggg sync`.

## Location

| Platform | Default path |
|----------|-------------|
| Linux    | `~/.local/share/ggg/` |
| macOS    | `~/Library/Application Support/ggg/` |
| Windows  | `%APPDATA%\ggg\` |

Set `GGG_CACHE_DIR` to use a different location. See [Environment variables](@/docs/reference/environment.md).

## Structure

```
<cache root>/
  godot/
    <release-key>/
      <executable and supporting files>
  deps/
    <url-hash>/
      <version-hash>/
        <dependency files>
        .ggg_dep_info.toml
```

### `godot/`

One subdirectory per Godot build, named after the release key (e.g. `4.3-stable`, `4.3-stable-mono`). Contains the extracted Godot binary and, for Mono builds, its supporting files.

### `deps/`

One subdirectory per dependency URL, named after a SHA-256 hash of the (normalised) URL. Within each URL directory, one subdirectory per version, named after the resolved version identifier.

#### Git dependencies

```
deps/
  <sha256(normalized-url)>/
    <40-char commit sha>/
      addons/
        ...
      .ggg_dep_info.toml
```

The URL is normalised before hashing: lowercased, protocol stripped, trailing `.git` and trailing slashes removed. This means the HTTPS and SSH forms of the same repository (`https://github.com/user/repo.git` and `git@github.com:user/repo.git`) resolve to the same cache directory and are never downloaded twice.

#### Archive and asset library dependencies

```
deps/
  <sha256(url)>/
    <sha256(archive-file)>/
      addons/
        ...
      .ggg_dep_info.toml
```

Archive URLs are used as-is (not normalised). The version subdirectory is named after the SHA-256 of the raw downloaded archive file, which is also stored in `ggg.lock`.

Asset library dependencies follow the same layout, using the resolved download URL.

#### `.ggg_dep_info.toml`

Every cache entry contains a small TOML metadata file recording where the entry came from. It is excluded when files are installed into your project.

For git dependencies:

```toml
name = "gut"
git  = "https://github.com/bitwes/Gut.git"
rev  = "v9.3.0"
sha  = "fbfabd5052e9..."
```

## Freeing space

The cache directory can be deleted safely at any time. It contains no unique data: everything in it can be re-derived from `ggg.toml` and `ggg.lock`.

To remove everything:

```
rm -rf ~/.local/share/ggg/        # Linux
rm -rf ~/Library/Application\ Support/ggg/  # macOS
rmdir /s %APPDATA%\ggg\           # Windows
```

To remove only the dependencies cache while keeping cached Godot binaries, delete `<cache root>/deps/` only.

## See also

- [Environment variables](@/docs/reference/environment.md): `GGG_CACHE_DIR`
- [`ggg sync`](@/docs/reference/commands/sync.md): populates the cache as a side effect
- [`ggg ls-dep`](@/docs/reference/commands/ls-dep.md): reads from the cache to show a dependency's source tree
