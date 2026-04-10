# Godot Goodie Grabber

**Godot Goodie Grabber** is a project manager for [Godot](https://godotengine.org/) games, inspired by [uv](https://github.com/astral-sh/uv) for Python. It pins a specific Godot version per project, manages asset dependencies from git sources, and provides a unified CLI to sync, edit, and run your project - so you never have to manually wrangle engine versions or addon installations again.

## Motivation

Godot projects suffer from the same version fragmentation problems as any other ecosystem: different projects need different engine versions, addons live in scattered git repos with no standard install mechanism, and there is no canonical way to reproduce a project's full environment from scratch. Godot Goodie Grabber solves this by treating the Godot binary itself as a versioned dependency and git repositories as the package source for addons.

## Concepts

### Project config (`ggg.toml`)

Each project has a `ggg.toml` at its root:

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

Godot Goodie Grabber downloads and caches Godot binaries keyed by version, similar to how uv manages Python interpreters. Binaries are stored in a shared cache directory (`~/.local/share/ggg/godot/` on Linux/macOS, `%APPDATA%\ggg\godot\` on Windows) so multiple projects that share a version reuse the same download.

### Dependency installation

Dependencies are plain git repositories. Godot Goodie Grabber checks them out (sparse or full, depending on the `target` path) into a local cache and links or copies the relevant subtree into the project. A `ggg.lock` file records the exact commit SHAs that were resolved, ensuring reproducible installs.

## Commands

```
ggg init              Create a ggg.toml in the current directory
ggg sync              Resolve and install all dependencies, download Godot if needed
ggg edit              Open the project in the pinned Godot editor
ggg run [-- <args>]   Run Godot against the project, forwarding extra arguments
ggg add <git-url>     Add a new dependency interactively
ggg remove <name>     Remove a dependency
ggg self update       Update Godot Goodie Grabber itself
```

## Workflow example

```sh
# Bootstrap a new project
ggg init                            # creates ggg.toml, prompts for Godot version
ggg sync                            # installs Godot 4.3.0 + all addons

# Day-to-day development
ggg edit                            # opens editor at the right version
ggg run -- --headless --export-release linux build/game.x86_64

# Onboarding a teammate
git clone https://github.com/your-org/your-game.git
cd your-game && ggg sync            # one command, reproducible environment
```

## Installation

```sh
cargo install ggg
```

Or download a pre-built binary from the [releases page](https://github.com/your-org/ggg/releases).

## Status

Early development. The config format and CLI surface are stable by design intent, but implementation is a work in progress.

## License

MIT OR Apache-2.0
