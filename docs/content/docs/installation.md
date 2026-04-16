+++
title = "Installation"
weight = 2
+++

## Linux and macOS

Run the installer script in your terminal:

```bash
curl -fsSL https://github.com/godotneers/ggg/releases/latest/download/ggg-installer.sh | sh
```

This downloads and runs a shell script that installs the `ggg` binary and a companion `ggg-update` binary into `~/.cargo/bin/`. If that directory is not already on your `PATH`, the installer will print a reminder.

### Supported platforms

| Platform | Architecture |
|----------|-------------|
| macOS | Apple Silicon (aarch64) |
| macOS | Intel (x86_64) |
| Linux | x86_64 (glibc) |

## Windows

Run the installer script in PowerShell:

```powershell
irm https://github.com/godotneers/ggg/releases/latest/download/ggg-installer.ps1 | iex
```

This installs `ggg.exe` and `ggg-update.exe` into `%USERPROFILE%\.cargo\bin\`. If that directory is not already on your `PATH`, the installer will print a reminder.

### Supported platforms

| Platform | Architecture |
|----------|-------------|
| Windows | x86_64 (MSVC) |

## Verifying the installation

After installation, open a new terminal and run:

```
ggg --version
```

## Updating

A `ggg-update` binary is installed alongside `ggg`. Run it at any time to update to the latest release:

```bash
ggg-update          # Linux / macOS
ggg-update.exe      # Windows
```

`ggg-update` fetches the latest release from GitHub and replaces the installed `ggg` binary in place. Your `ggg.toml` files and cache are not affected.

## Manual installation

Pre-built archives for all supported platforms are attached to every [GitHub release](https://github.com/godotneers/ggg/releases). Download the archive for your platform, extract it, and place the `ggg` binary somewhere on your `PATH`.

## Uninstalling

Delete the `ggg` and `ggg-update` binaries from your install directory (`~/.cargo/bin/` on Linux and macOS, `%USERPROFILE%\.cargo\bin\` on Windows). The shared cache can be removed separately if you want to free disk space. See the [cache reference](@/docs/reference/cache.md) for per-platform locations and what the cache contains.
