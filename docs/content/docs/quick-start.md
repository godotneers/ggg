+++
title = "Quick Start"
weight = 1
+++

Get up and running with {{ tool_name() }} in a few minutes.

## Prerequisites

- **Godot project**: You need an existing `project.godot` file, or create a new project via the Godot editor first.
- **Internet access**: {{ tool_name() }} downloads the Godot binary and git dependencies on first use.

## Installation

Download a pre-built binary from the [releases page](https://github.com/derkork/ggg/releases) and put it somewhere on your `PATH`, or install via Cargo:

```bash
cargo install ggg
```

The binary is named `{{ cli() }}`.

## Initialise a Project

Navigate to your Godot project's root directory (where `project.godot` lives) and run:

```bash
ggg init
```

This creates a `ggg.toml` pre-filled with the latest stable Godot version. Edit it to match your project:

```toml
[project]
godot = "4.3.0"
```

## Add Dependencies

Add an addon by passing its git URL:

```bash
ggg add https://github.com/bitwes/Gut.git
```

{{ tool_name() }} will prompt for a name, revision, and install path, then add an entry to `ggg.toml`:

```toml
[[dependency]]
name   = "gut"
git    = "https://github.com/bitwes/Gut.git"
rev    = "v9.3.0"
target = "addons/gut"
```

You can also edit `ggg.toml` directly - `{{ cli() }} sync` will pick up any changes.

## Sync

Install Godot and all dependencies:

```bash
ggg sync
```

{{ tool_name() }} will:

1. Download and cache the Godot binary declared in `[project]`
2. Clone or fetch each dependency and check out the declared `rev`
3. Copy (or link) the addon files into the `target` path inside your project
4. Write a `ggg.lock` file recording the exact commit SHA for each dependency

Commit both `ggg.toml` and `ggg.lock` to your repository.

## Open the Editor

```bash
ggg edit
```

Launches the Godot editor at the exact version pinned in `ggg.toml`. No hunting for the right binary.

## Run the Project

```bash
ggg run
```

Runs Godot against the project. Pass additional arguments after `--`:

```bash
ggg run -- --headless --export-release linux build/game.x86_64
```

## Onboarding a Teammate

```bash
git clone https://github.com/your-org/your-game.git
cd your-game
ggg sync
ggg edit
```

That's it. `{{ cli() }} sync` reproduces the full environment from `ggg.lock` - same Godot binary, same addon revisions, every time.
