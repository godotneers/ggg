+++
title = "Configuration (ggg.toml)"
weight = 1
+++

Every project managed by Godot Goodie Grabber has a `ggg.toml` file at its root, alongside `project.godot`. This file is the single source of truth for the project's Godot version and addon dependencies.

A companion `ggg.lock` file is generated automatically by [`ggg sync`](@/docs/reference/commands/sync.md) and should be committed alongside `ggg.toml`. It records the exact versions that were resolved, ensuring that every contributor and every CI run installs bit-for-bit identical dependencies.

## `[project]`

Declares the Godot engine version for this project.

```toml
[project]
godot = "4.3-stable"
```

### Fields

#### `godot`

**Required.** The Godot build to use, as a string in `VERSION-FLAVOR` or `VERSION-FLAVOR-mono` format.

```toml
godot = "4.3-stable"        # standard build
godot = "4.3-stable-mono"   # Mono (C#) build
godot = "4.3.1-stable"      # patch release
godot = "4.4-rc2"           # release candidate
```

`VERSION` is `MAJOR.MINOR` or `MAJOR.MINOR.PATCH`. `FLAVOR` is the release stage: `stable`, `rc1`, `beta2`, `dev4`, and so on. Appending `-mono` selects the Mono (C#) build.

`ggg sync` will download this exact binary if it is not already present in the shared cache. [`ggg edit`](@/docs/reference/commands/edit.md) and [`ggg run`](@/docs/reference/commands/run.md) will invoke it.

---

## `[sync]`

Optional table that controls sync behaviour. Can be omitted entirely when the defaults are acceptable.

```toml
[sync]
force_overwrite = ["**/*.import", "**/*.uid"]
```

### Fields

#### `force_overwrite`

**Optional.** A list of glob patterns matched against project-relative paths. Any installed file whose path matches one of these patterns is unconditionally overwritten by `ggg sync`, bypassing conflict detection.

```toml
[sync]
force_overwrite = ["**/*.import", "**/*.uid"]
```

This is primarily useful for files that the Godot editor rewrites automatically -- such as `.import` metadata and `.uid` files -- which would otherwise trigger a spurious conflict on every sync. See the [`ggg sync` reference](@/docs/reference/commands/sync.md) for a full explanation of conflict detection and the `--force` flag.

---

## `[[dependency]]`

Declares an addon dependency. Each entry in the array describes one addon.

Dependencies come in two flavours:

- **Git dependencies** fetch a specific revision from a git repository. Use these for addons hosted on GitHub or any other git host.
- **Archive dependencies** download a pre-built archive (`.zip`, `.tar.gz`, `.tgz`) directly from a URL. Use these for addons distributed as release assets rather than source repositories.

The `git` and `url` fields are mutually exclusive: every dependency must have exactly one of them. Fields that belong to one source type are rejected on the other.

After `ggg sync` runs, each dependency's resolved identity is recorded in `ggg.lock`. Subsequent syncs use the locked value unless the dependency changes in `ggg.toml`.

### Git dependency

```toml
[[dependency]]
name = "gut"
git  = "https://github.com/bitwes/Gut.git"
rev  = "v9.3.0"
```

### Archive dependency

```toml
[[dependency]]
name             = "debug-draw"
url              = "https://github.com/DmitriySalnikov/godot_debug_draw_3d/releases/download/1.7.3/debug-draw-3d_1.7.3.zip"
sha256           = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
strip_components = 1
```

Multiple dependencies are declared by repeating the `[[dependency]]` header:

```toml
[[dependency]]
name = "gut"
git  = "https://github.com/bitwes/Gut.git"
rev  = "v9.3.0"

[[dependency]]
name = "phantom-camera"
git  = "https://github.com/ramokz/phantom-camera.git"
rev  = "v0.8"
```

### Fields

#### `name`

**Required.** A short identifier for this dependency. Used in the lock file, in CLI output, and as the argument to [`ggg remove`](@/docs/reference/commands/remove.md).

```toml
name = "gut"
```

Must be unique within `ggg.toml`. Use lowercase letters, digits, and hyphens.

---

#### `git`

**Git deps only. Required when using a git source.** The URL of the git repository that contains the addon. HTTPS and SSH URLs are both accepted. Mutually exclusive with `url`.

```toml
git = "https://github.com/bitwes/Gut.git"
git = "git@github.com:bitwes/Gut.git"
```

---

#### `rev`

**Git deps only. Required when `git` is set.** The revision to check out. Can be a tag, a branch name, or a full 40-character commit SHA.

```toml
rev = "v9.3.0"       # tag - recommended for stable addons
rev = "main"         # branch - always resolves to the latest commit on that branch
rev = "a1b2c3d4..."  # full commit SHA - most reproducible
```

When `rev` is a tag or branch, `ggg sync` resolves it to a commit SHA and records that SHA in `ggg.lock`. Subsequent syncs use the locked SHA unless you explicitly update the dependency.

---

#### `url`

**Archive deps only. Required when using an archive source.** HTTPS URL of a pre-built archive. The URL must end in `.zip`, `.tar.gz`, or `.tgz`. Mutually exclusive with `git`.

```toml
url = "https://github.com/example/addon/releases/download/v1.0/addon.zip"
```

---

#### `sha256`

**Archive deps only. Optional.** The expected SHA-256 hex digest of the downloaded archive. When present, `ggg sync` verifies every download against this hash and fails if they do not match.

```toml
sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
```

Strongly recommended: it catches accidental or malicious changes to the archive at the source URL.

---

#### `strip_components`

**Archive deps only. Optional.** Number of leading path components to remove from archive entries before installing, equivalent to `tar --strip-components`. Defaults to `0`.

```toml
strip_components = 1
```

Many release archives wrap their contents in a top-level directory (e.g. `addon-v1.0/addons/...`). Setting `strip_components = 1` removes that wrapper so the `addons/` tree lands at the right level.

Stripping is applied before `map` entries are evaluated, so `from` paths in `map` refer to the already-stripped paths. See the [`ggg sync` reference](@/docs/reference/commands/sync.md) for a worked example.

---

#### `map`

**Optional.** A list of path mappings that control which parts of the source are installed into your project, and where.

Each entry is an inline table with a `from` key and an optional `to` key:

```toml
map = [
    { from = "addons/some_addon" },
    { from = "examples/", to = "examples/some_addon" },
]
```

- **`from`** - a path within the source tree (file or directory). For archive deps, this is the path after `strip_components` has been applied.
- **`to`** - the destination path inside your project, relative to the project root. When omitted, defaults to the same value as `from`.

When `map` is omitted entirely, the entire source tree is copied into the project root as-is. In most cases this is what you want as most Godot addons ship only the relevant files. Some addons ship additional examples or README files which you may not want. In these cases adding mappings will allow you to strip those out.

When `map` is present, **only the listed paths are copied** -- everything else in the source tree is ignored. Given the example above:

```toml
map = [
    { from = "addons/some_addon" },
    { from = "examples/", to = "examples/some_addon" },
]
```

This installs `addons/some_addon` from the source into `addons/some_addon` in your project, and copies the `examples/` directory into `examples/some_addon`. Any other files in the repository (README, CI config, tests, etc.) are not installed.

> **Note:** Changing the destination path with `to` does not update any internal references inside the copied files. If an addon hard-codes paths to its own assets (scripts, scenes, resources), moving it to a different location will break those references. `ggg` will not fix this for you. In practice this is mainly a concern when remapping the addon directory itself; remapping unrelated extras like examples is usually safe.

Trailing slashes on directory paths are optional and have no effect.

---

## Full example

```toml
[project]
godot = "4.3-stable"

[sync]
# Godot rewrites these files on every project open; skip conflict detection for them
force_overwrite = ["**/*.import", "**/*.uid"]

# Git dep - single mapping, destination equals source so `to` is omitted
[[dependency]]
name = "gut"
git  = "https://github.com/bitwes/Gut.git"
rev  = "v9.3.0"
map  = [{ from = "addons/gut" }]

# Git dep - multiple mappings: install the addon and also grab the examples
[[dependency]]
name = "phantom-camera"
git  = "https://github.com/ramokz/phantom-camera.git"
rev  = "v0.8"
map  = [
    { from = "addons/phantom_camera" },
    { from = "examples/", to = "examples/phantom_camera" },
]

# Archive dep - strip the top-level wrapper directory, install just the addon
[[dependency]]
name             = "debug-draw"
url              = "https://github.com/example/debug_draw_3d/releases/download/v1.0/debug_draw_3d.zip"
sha256           = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
strip_components = 1
map              = [{ from = "addons/debug_draw_3d" }]

# Git dep - no map, copies the entire repository into the project root
[[dependency]]
name = "dialogue-manager"
git  = "https://github.com/nathanhoad/godot_dialogue_manager.git"
rev  = "v2.34.0"
```
