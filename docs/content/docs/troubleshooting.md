+++
title = "Troubleshooting"
weight = 9
+++

## Known limitations

### Symlinks in git repositories are installed as plain files

If a dependency contains symlinks, ggg installs them as regular files containing the link target path as text rather than creating actual symlinks. This can cause issues with addons that rely on symlinked files.

This is a known limitation. If you encounter it, check whether the addon works correctly and report the issue if not.

### Stale files not cleaned up after deleting `.ggg.state`

ggg tracks every file it installs in `.ggg.state`. If this file is deleted before a sync that removes a dependency, the old files are not cleaned up automatically. A warning is printed when `.ggg.state` is missing. Run `ggg sync` once to restore the state file, then run it again to clean up the stale files.

### The same repository is added under two different names

`ggg add` checks for duplicate dependency names but not duplicate URLs. If you add the same repository twice under different names (or using both the HTTPS and SSH form of the URL), both entries will pass validation but `ggg sync` will fail with a file conflict error.

Remove the duplicate with `ggg remove <name>` and run `ggg sync` again.

### Comments inside `[[dependency]]` blocks are lost after `ggg add` or `ggg remove`

`ggg add` and `ggg remove` rewrite the `[[dependency]]` section of `ggg.toml`. Any comments you have placed inside a `[[dependency]]` block are lost in the process. Comments outside the dependency blocks (such as at the top of the file or inside `[project]`) are preserved.

## Getting help

If you run into a problem not covered here, please [open an issue on GitHub](https://github.com/godotneers/ggg/issues). Include the command you ran, the error message, and your `ggg.toml` if relevant.
