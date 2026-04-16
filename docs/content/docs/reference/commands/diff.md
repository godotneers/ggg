+++
title = "ggg diff"
weight = 8
+++

```
ggg diff [<file>]
```

Shows a unified diff between the version of each addon file installed by `ggg sync` and the version currently on disk. Only files that `ggg` installed and that have since been modified are shown.

Use this to review local edits to addon files before deciding whether to keep them or discard them with [`ggg sync --force`](@/docs/reference/commands/sync.md).

## Usage

```bash
ggg diff
```

Shows diffs for all modified addon files across all dependencies.

```bash
ggg diff addons/gut/plugin.cfg
```

Limits output to a single file. The path should be relative to the project root.

## Example output

```
Diff for gut (v9.3.0 -> fbfabd5052e9):

  addons/gut/plugin.cfg

@@ -1,5 +1,5 @@
 [plugin]
 name="Gut"
-version="9.3.0"
+version="9.3.1-local-patch"
 script="gut_plugin.gd"
```

Output is coloured when writing to a terminal. Set the `NO_COLOR` environment variable to disable colours.

Binary files are listed but their content is not shown.

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | No modified files found. |
| `1` | One or more addon files have been modified locally. |

The non-zero exit code on modifications makes `ggg diff` useful in CI to enforce that addon files are not edited directly.

## Notes

- Only files that `ggg` installed (recorded in `.ggg.state`) and that have since been modified are shown. Untracked files and files owned by other tools are not included.
- Binary files are noted but not diffed.
- If `.ggg.state` is missing, `ggg diff` still runs but may report more files as modified than expected. Running `ggg sync` will restore the state file.

## See also

- [`ggg sync`](@/docs/reference/commands/sync.md): apply the latest dependency versions, with `--force` to discard local changes
