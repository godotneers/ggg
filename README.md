# gragen

**gragen** (the Godot Kraken) is a project manager for [Godot](https://godotengine.org/) games, inspired by [uv](https://github.com/astral-sh/uv) for Python. It pins a specific Godot version per project, manages asset dependencies from git sources, and provides a unified CLI to sync, edit, and run your project — so you never have to manually wrangle engine versions or addon installations again.

## Motivation

Godot projects suffer from the same version fragmentation problems as any other ecosystem: different projects need different engine versions, addons live in scattered git repos with no standard install mechanism, and there is no canonical way to reproduce a project's full environment from scratch. gragen solves this by treating the Godot binary itself as a versioned dependency and git repositories as the package source for addons.

## Concepts

### Project config (`gragen.toml`)

Each project has a `gragen.toml` at its root:

```toml
[project]
godot = "4.3.0"          # Godot version to use for this project

[[dependency]]
name   = "gut"
git    = "https://github.com/bitwes/Gut.git"
rev    = "v9.3.0"        # tag, branch, or commit SHA
target = "addons/gut"    # where inside the project to install it

[[dependency]]
name   = "phantom-camera"
git    = "https://github.com/ramokz/phantom-camera.git"
rev    = "main"
target = "addons/phantom_camera"
```

### Godot version management

gragen downloads and caches Godot binaries keyed by version, similar to how uv manages Python interpreters. Binaries are stored in a shared cache directory (`~/.local/share/gragen/godot/` on Linux/macOS, `%APPDATA%\gragen\godot\` on Windows) so multiple projects that share a version reuse the same download.

### Dependency installation

Dependencies are plain git repositories. gragen checks them out (sparse or full, depending on the `target` path) into a local cache and links or copies the relevant subtree into the project. A `gragen.lock` file records the exact commit SHAs that were resolved, ensuring reproducible installs.

## Commands

```
gg init              Create a gragen.toml in the current directory
gg sync              Resolve and install all dependencies, download Godot if needed
gg edit              Open the project in the pinned Godot editor
gg run [-- <args>]   Run Godot against the project, forwarding extra arguments
gg add <git-url>     Add a new dependency interactively
gg remove <name>     Remove a dependency
gg self update       Update gragen itself
```

## Workflow example

```sh
# Bootstrap a new project
gg init                            # creates gragen.toml, prompts for Godot version
gg sync                            # installs Godot 4.3.0 + all addons

# Day-to-day development
gg edit                            # opens editor at the right version
gg run -- --headless --export-release linux build/game.x86_64

# Onboarding a teammate
git clone https://github.com/your-org/your-game.git
cd your-game && gg sync            # one command, reproducible environment
```

## Installation

```sh
cargo install gragen
```

Or download a pre-built binary from the [releases page](https://github.com/your-org/gragen/releases).

## Status

Early development. The config format and CLI surface are stable by design intent, but implementation is a work in progress.

## License

MIT OR Apache-2.0
