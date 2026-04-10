+++
title = "Configuration (ggg.toml)"
weight = 1
+++

Every project managed by {{ tool_name() }} has a `ggg.toml` file at its root, alongside `project.godot`. This file is the single source of truth for the project's Godot version and addon dependencies.

A companion `ggg.lock` file is generated automatically by `{{ cli() }} sync` and should be committed alongside `ggg.toml`. It records the exact commit SHAs that were resolved, ensuring that every contributor and every CI run installs bit-for-bit identical dependencies.

## `[project]`

Declares the Godot engine version for this project.

```toml
[project]
godot = "4.3.0"
```

### Fields

#### `godot`

**Required.** The Godot version to use, as a string in `MAJOR.MINOR.PATCH` format.

```toml
godot = "4.3.0"
```

`{{ cli() }} sync` will download this exact binary if it is not already present in the shared cache. `{{ cli() }} edit` and `{{ cli() }} run` will invoke it.

Stable releases use the three-part version string. Godot release candidates and dev snapshots are not supported.

---

## `[[dependency]]`

Declares an addon dependency sourced from a git repository. Each entry in the array describes one addon.

```toml
[[dependency]]
name = "gut"
git  = "https://github.com/bitwes/Gut.git"
rev  = "v9.3.0"
map  = [
    { from = "addons/gut" },
    { from = "examples/", to = "examples/gut" },
]
```

Multiple dependencies are declared by repeating the `[[dependency]]` header:

```toml
[[dependency]]
name = "gut"
git  = "https://github.com/bitwes/Gut.git"
rev  = "v9.3.0"
map  = [{ from = "addons/gut" }]

[[dependency]]
name = "phantom-camera"
git  = "https://github.com/ramokz/phantom-camera.git"
rev  = "v0.8"
map  = [{ from = "addons/phantom_camera" }]
```

### Fields

#### `name`

**Required.** A short identifier for this dependency. Used in the lock file, in CLI output, and as the argument to `{{ cli() }} remove`.

```toml
name = "gut"
```

Must be unique within `ggg.toml`. Use lowercase letters, digits, and hyphens.

---

#### `git`

**Required.** The URL of the git repository that contains the addon. HTTPS and SSH URLs are both accepted.

```toml
git = "https://github.com/bitwes/Gut.git"
git = "git@github.com:bitwes/Gut.git"
```

---

#### `rev`

**Required.** The revision to check out. Can be a tag, a branch name, or a full 40-character commit SHA.

```toml
rev = "v9.3.0"       # tag - recommended for stable addons
rev = "main"         # branch - always resolves to the latest commit on that branch
rev = "a1b2c3d4..."  # full commit SHA - most reproducible
```

When `rev` is a tag or branch, `{{ cli() }} sync` resolves it to a commit SHA and records that SHA in `ggg.lock`. Subsequent syncs use the locked SHA unless you explicitly update the dependency.

---

#### `map`

**Optional.** A list of path mappings that control which parts of the repository are installed into your project, and where.

Each entry is an inline table with a `from` key and an optional `to` key:

```toml
map = [
    { from = "addons/gut" },
    { from = "examples/", to = "examples/gut" },
]
```

- **`from`** - a path within the git repository (file or directory)
- **`to`** - the destination path inside your project, relative to the project root. When omitted, defaults to the same value as `from`

When `map` is omitted entirely, the entire repository contents are copied into the project root. In most cases you will want to specify at least one mapping to avoid polluting your project with README files, CI configs, and other repository metadata.

Trailing slashes on directory paths are optional and have no effect.

---

## Full example

```toml
[project]
godot = "4.3.0"

# Single mapping, destination equals source so `to` is omitted
[[dependency]]
name = "gut"
git  = "https://github.com/bitwes/Gut.git"
rev  = "v9.3.0"
map  = [{ from = "addons/gut" }]

# Multiple mappings - install the addon and also grab the examples
[[dependency]]
name = "phantom-camera"
git  = "https://github.com/ramokz/phantom-camera.git"
rev  = "v0.8"
map  = [
    { from = "addons/phantom_camera" },
    { from = "examples/", to = "examples/phantom_camera" },
]

# No map - copies the entire repository into the project root
[[dependency]]
name = "dialogue-manager"
git  = "https://github.com/nathanhoad/godot_dialogue_manager.git"
rev  = "v2.34.0"
```
