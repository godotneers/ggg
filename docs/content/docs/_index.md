+++
title = "Documentation"
sort_by = "weight"
template = "section.html"
+++

Managing Godot projects across a team is messier than it should be. Different contributors run different engine versions. Addons are committed wholesale into the repository or documented in a README that nobody reads. There is no standard way to reproduce the full project environment from scratch.

Godot Goodie Grabber fixes this.

## What is Godot Goodie Grabber?

Godot Goodie Grabber is a project manager for Godot games, inspired by [uv](https://github.com/astral-sh/uv) for Python. It gives every project a `ggg.toml` file that declares two things:

- **The Godot version** the project requires
- **A list of addon dependencies** sourced from git repositories or pre-built archives

From there, a single command - `ggg sync` - downloads the right Godot binary and installs every declared addon at the exact revision pinned in `ggg.lock`. No more README instructions, no more committed vendor trees, no more "works on my machine" engine mismatches.

## CLI Overview

| Command | What it does |
|---|---|
| [`ggg init`](@/docs/reference/commands/init.md) | Create a `ggg.toml` in the current directory |
| [`ggg add`](@/docs/reference/commands/add.md) | Add a dependency (git or archive) |
| [`ggg remove`](@/docs/reference/commands/remove.md) | Remove a dependency |
| [`ggg sync`](@/docs/reference/commands/sync.md) | Download Godot + install all dependencies |
| [`ggg edit`](@/docs/reference/commands/edit.md) | Open the project in the pinned Godot editor |
| [`ggg run`](@/docs/reference/commands/run.md) | Run the project with the pinned Godot binary |
| [`ggg ls-dep`](@/docs/reference/commands/ls-dep.md) | List a dependency's source tree before syncing |
| [`ggg diff`](@/docs/reference/commands/diff.md) | Show local changes to installed addon files |

## What's Next?

Follow the [Quick Start](@/docs/quick-start.md) guide to set up a project in a few minutes.
