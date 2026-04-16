+++
title = "ggg init"
weight = 1
+++

```
ggg init
```

Run `ggg init` once at the start of a project to set it up for use with Godot Goodie Grabber. It creates a `ggg.toml` file in the current directory and makes a few small changes to your project to keep things tidy.

## Usage

Navigate to your project directory and run:

```bash
ggg init
```

You will be prompted to choose a Godot version from a list of stable releases. Type to filter the list. If a `project.godot` is already present, the version recorded there is pre-selected so you can just press Enter to confirm.

If your project uses the Mono (C#) build of Godot and there is no `project.godot` yet, you will also be asked whether to use the Mono build. If `project.godot` already exists this question is skipped; the answer is read from the file automatically.

Once you confirm, `ggg init` creates your `ggg.toml`:

```toml
[project]
godot = "4.3-stable"
```

## What changes in your project

- **`ggg.toml` is created** with the Godot version you selected. This is the file you will edit to add and configure dependencies.
- **`.gitignore` is updated** to exclude `.ggg.state`. This file is managed by Godot Goodie Grabber and should not be committed (it is specific to each checkout). If `.gitignore` does not exist it is created.
- **`project.godot` is created** if none was present. The generated file is minimal, containing just enough for Godot to open the project in the editor. You can open it with `ggg edit` and configure your project from there as normal.

## Next steps

Once `ggg.toml` exists, use [`ggg add`](@/docs/reference/commands/add.md) to add dependencies and [`ggg sync`](@/docs/reference/commands/sync.md) to install them.

Commit `ggg.toml` to your repository. Your teammates can then run `ggg sync` to get the same Godot version and the same addon files without any manual setup.

## Notes

- Fails immediately if `ggg.toml` already exists. To change the Godot version after initialising, edit `ggg.toml` directly.
- Only stable Godot releases are listed. To use a release candidate or dev build, set the `godot` field in `ggg.toml` by hand after running `ggg init`. See the [configuration reference](@/docs/reference/configuration.md) for the accepted format.
