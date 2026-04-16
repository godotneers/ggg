+++
title = "Quick Start"
weight = 1
+++

In this guide we'll use Godot Goodie Grabber to manage the dependencies and Godot version of a Godot project. The whole thing takes about five minutes.

## Installation

First, we need to get Godot Goodie Grabber onto our machine. Head over to the [installation guide](@/docs/installation.md) for instructions, then come back here.

## Initialize the project

Let's open a terminal, navigate to the directory where we want to create your project, and run:

```bash
ggg init
```

Godot Goodie Grabber fetches the list of available Godot releases and shows us an interactive picker so we can choose the version we want. If a `project.godot` file is already present, it pre-selects the version that matches. Once we confirm, Godot Goodie Grabber creates a `ggg.toml`:

```toml
[project]
godot = "4.6.2"
```

If there was no `project.godot` yet, Godot Goodie Grabber also creates a minimal one for the selected Godot version so the project opens correctly in the editor.

Godot Goodie Grabber also adds `.ggg.state` to `.gitignore` automatically. The `.ggg.state` file tracks which files Godot Goodie Grabber has installed into the project. It should not be committed to the repository, which is why it goes into `.gitignore`.

## Add a dependency

We can add an addon with [`ggg add`](@/docs/reference/commands/add.md). Let's add GUT, the Godot unit testing framework:

```bash
ggg add https://github.com/bitwes/Gut.git
```

Godot Goodie Grabber detects this is a Git URL and asks us for a revision (a tag, branch, or commit SHA) and a short name. We pick `main` as the revision and accept the suggested name `gut`. Godot Goodie Grabber then verifies the revision exists on the remote and adds the entry to `ggg.toml`:

```
Added "gut" (main) resolved to fbfabd5052e9
Run `ggg sync` to install it.
```

The new entry in `ggg.toml` looks like this:

```toml
[[dependency]]
name = "gut"
git  = "https://github.com/bitwes/Gut.git"
rev  = "main"
```

## Sync

Now let's install everything:

```bash
ggg sync
```

Godot Goodie Grabber downloads the Godot release declared in `[project]` into a shared cache on our machine. Then it fetches each dependency, copies the files into the project, and writes a `ggg.lock` file. For example, we can see the output of [`ggg sync`](@/docs/reference/commands/sync.md) for GUT:

```
gut - downloading...
Fetched gut
gut (resolved fbfabd5052e9): installed 257 files (257 total)
```

The `ggg.lock` file records the exact commit SHA resolved so that everytime you, a teammate, or your CI system runs `ggg sync` the bit-for-bit identical files are fetched. Because `ggg` will fetch the correct addon versions for us, we actually don't need to commit them with our project, so we can add the `addons` folder to our project's `.gitignore` file. 

```bash
echo "addons/" >> .gitignore
```

Finally, let's commit both `ggg.toml` and `ggg.lock` to our repository, so every:

```bash
git add ggg.toml ggg.lock
git commit -m "Add ggg project configuration"
```

## Editing the project

Now we can open the project in godot by typing [`ggg edit`](@/docs/reference/commands/edit.md) in the terminal. This will launch the exact Godot version we gave in `ggg.toml`. Finally, we need to enable the _GUT_ addon in the editor's project settings, as this is specific to the addon and Godot Goodie Grabber cannot do it for us. 


## What's next
- **Run the project** - [`ggg run`](@/docs/reference/commands/run.md) starts Godot in the project.
- **More dependency types** - besides git repositories, Godot Goodie Grabber can also install pre-built archive dependencies. See the [`ggg add` reference](@/docs/reference/commands/add.md) for details
- **Configuration reference** - the full list of fields available in `ggg.toml` is in the [configuration reference](@/docs/reference/configuration.md)
