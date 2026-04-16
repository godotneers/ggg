+++
title = "ggg edit"
weight = 5
+++

```
ggg edit [<godot-args>...]
```

Opens the current project in the Godot editor using the version declared in `ggg.toml`. If that version is not yet cached, it is downloaded first.

This is the standard way to open a project managed by Godot Goodie Grabber. Using `ggg edit` instead of launching Godot directly ensures you are always using the exact version the project requires.

## Usage

```bash
ggg edit
```

Run this from your project directory. Godot opens with the current project loaded. When you close the editor, the terminal prompt returns.

Any arguments you add after `ggg edit` are forwarded verbatim to Godot:

```bash
ggg edit --verbose
ggg edit --rendering-method gl_compatibility
```

## Notes

- Requires a `ggg.toml` in the current directory.
- Downloads and caches the declared Godot version on first run. Subsequent runs start immediately from the cache.
- The Godot binary is shared across all your projects in a single cache directory. See the [cache reference](@/docs/reference/cache.md) for the cache location and how to manage it.
- `ggg edit` does not run `ggg sync` first. If you have just added or updated dependencies, run [`ggg sync`](@/docs/reference/commands/sync.md) before opening the editor.

## See also

- [`ggg run`](@/docs/reference/commands/run.md): runs the project without opening the editor
- [`ggg sync`](@/docs/reference/commands/sync.md): installs dependencies before editing
