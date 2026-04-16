+++
title = "ggg run"
weight = 6
+++

```
ggg run [<godot-args>...]
```

Runs the current project using the Godot version declared in `ggg.toml`. If that version is not yet cached, it is downloaded first.

Unlike [`ggg edit`](@/docs/reference/commands/edit.md), this launches Godot in game mode rather than opening the editor. Use it to run your project from the terminal, for example in a CI environment or to quickly test without opening the full editor.

## Usage

```bash
ggg run
```

Run this from your project directory. Any arguments you add are forwarded verbatim to Godot:

```bash
ggg run --headless
ggg run --headless --script res://tests/run_tests.gd
```

## Notes

- Requires a `ggg.toml` in the current directory.
- Downloads and caches the declared Godot version on first run. Subsequent runs start immediately from the cache.
- Does not run `ggg sync` first. If you have just added or updated dependencies, run [`ggg sync`](@/docs/reference/commands/sync.md) beforehand.

## See also

- [`ggg edit`](@/docs/reference/commands/edit.md): opens the project in the Godot editor instead
- [`ggg sync`](@/docs/reference/commands/sync.md): installs dependencies before running
