+++
title = "ggg add"
weight = 2
+++

```
ggg add git <url>[@<rev>] [--name <name>] [--yes]
ggg add archive <url> [--name <name>] [--strip-components <n>] [--sha256 <hash>] [--yes]
ggg add asset-library [<query-or-id>] [--id <N>] [--name <name>] [--strip-components <n>] [--yes]
ggg add asset-store <publisher/slug[:version]|query> [--name <name>] [--strip-components <n>] [--yes]
ggg add asset <publisher/slug[:version]|query>   # alias for asset-store
ggg add <url-or-query>
```

Adds a new dependency to `ggg.toml`. Run this from your project directory whenever you want to include a new addon. After adding, run [`ggg sync`](@/docs/reference/commands/sync.md) to install it.

There are four kinds of dependency: **git** (sourced from a repository), **archive** (a pre-built `.zip`, `.tar.gz`, or `.tgz` downloaded from a URL), **asset-library** (sourced from the [Godot Asset Library](https://godotengine.org/asset-library/) by numeric ID), and **asset-store** (sourced from the [Godot Asset Store](https://store.godotengine.org/), pinned to a specific release). `asset` is an alias for `asset-store`.

If you pass a URL, reference, or search term directly without a keyword, `ggg add` detects the type from the input and adds the dependency through the matching path:

| Input | Type |
| --- | --- |
| ends in `.zip`, `.tar.gz`, `.tgz` | `archive` |
| `publisher_slug/asset_slug[:version]` | `asset-store` |
| contains `://`, ends in `.git`, or is SCP-style (`user@host:path`) | `git` |
| a plain number | `asset-library` (by ID) |
| anything else | searches the Asset Store **and** Asset Library together |

The Godot version declared in `ggg.toml` filters every search and release selection to versions compatible with your project.

## `ggg add git`

```bash
ggg add git https://github.com/bitwes/Gut.git@v9.3.0
```

Adds a git dependency. You will be prompted for anything not supplied on the command line.

**URL:** the HTTPS or SSH URL of the repository. Both formats are accepted:

```
https://github.com/bitwes/Gut.git
git@github.com:bitwes/Gut.git
```

**Revision:** the branch, tag, or full commit SHA to install. Append it to the URL with `@`, or enter it when prompted. Defaults to `main` if you press Enter without typing anything. Examples:

```
v9.3.0       tag
main         branch
a1b2c3d4...  full commit SHA
```

`ggg add` contacts the remote to verify the revision exists before writing anything. A short SHA of the resolved commit is printed as confirmation. If the revision cannot be found the command fails without modifying `ggg.toml`.

**`--name`:** override the dependency name instead of accepting the suggested one or being prompted.

**`--yes`:** skips all prompts and uses the suggested name (derived from the repository name, lowercased, `.git` suffix removed). Requires the URL to include a revision (`url@rev` form); the command fails if the revision is missing.

### Example output

```
$ ggg add git https://github.com/bitwes/Gut.git@v9.3.0
? Name · gut
Added "gut" (v9.3.0) resolved to fbfabd5052e9
Run `ggg sync` to install it.
```

The resulting entry in `ggg.toml`:

```toml
[[dependency]]
name = "gut"
git  = "https://github.com/bitwes/Gut.git"
rev  = "v9.3.0"
```

## `ggg add archive`

```bash
ggg add archive https://example.com/addon-v1.0.zip --name my-addon
```

Adds an archive dependency. Unlike git dependencies, no network request is made at add time; the archive is downloaded on the next `ggg sync`.

**URL:** a direct download link to a `.zip`, `.tar.gz`, or `.tgz` file.

**`--name`:** override the dependency name. If omitted (and `--yes` is not set) a name is proposed based on the URL's filename — `debug_draw_3d.zip` becomes `debug-draw-3d` — and you are prompted to accept or change it.

**`--sha256`:** the expected SHA-256 hex digest of the archive. Strongly recommended: `ggg sync` will verify every download against this hash and fail if they do not match, protecting against accidental or malicious changes at the source URL. If omitted, a reminder is printed.

**`--strip-components`:** number of leading path components to strip from the archive contents before installing (equivalent to `tar --strip-components`). Useful when the archive wraps everything in a top-level directory. See the [configuration reference](@/docs/reference/configuration.md) for details.

**`--yes`:** accept the inferred name without prompting. So `ggg add <url>.zip --yes` works without `--name`:

```bash
ggg add archive https://example.com/debug_draw_3d.zip --yes
ggg add https://example.com/debug_draw_3d.zip --yes   # same, bare form
```

### Example output

```
$ ggg add archive https://example.com/debug_draw_3d.zip --name debug-draw
Added "debug-draw" from https://example.com/debug_draw_3d.zip
Tip: add a sha256 = "<hash>" field to verify the download integrity.
Run `ggg sync` to install it.
```

## `ggg add asset-library`

```bash
ggg add asset-library gut
ggg add asset-library --id 54
ggg add asset-library 54
ggg add 54
```

Searches the [Godot Asset Library](https://godotengine.org/asset-library/) and adds the selected asset as a dependency. The Godot version declared in `ggg.toml` is used to filter results to compatible assets.

**Query:** a name or keyword to search for. If you already know the numeric asset ID, pass it directly (with or without the keyword) or use `--id` — both skip the search.

**`--id`:** bypass the search and add the asset with this ID directly. Useful when you have the asset page URL from the website.

**`--name`:** override the dependency name instead of accepting the suggested one or being prompted.

**`--yes`:** accept the suggested dependency name.

Search results follow the standard disambiguation flow described below.

### Example output

```
$ ggg add asset-library gut
? Name · gut
Found: GUT - Godot Unit Testing (v9.3.0)
  Author:  bitwes
  License: MIT
  Browse:  https://godotengine.org/asset-library/asset/54

Added "gut" (asset #54). Run `ggg sync` to install.
```

The resulting entry in `ggg.toml`:

```toml
[[dependency]]
name                = "gut"
asset_library_id    = 54
```

Asset library dependencies default to `strip_components = 1` because the asset library packages every asset inside a top-level folder. Override this in `ggg.toml` if needed.

## `ggg add asset-store`

```bash
ggg add asset-store souleat/godot-xoshiro256-plus-plus
ggg add asset-store souleat/godot-xoshiro256-plus-plus:1.1.0
ggg add asset-store decal-co          # keyword search
ggg add asset souleat/godot-xoshiro256-plus-plus --yes   # `asset` is an alias
```

Adds a [Godot Asset Store](https://store.godotengine.org/) dependency. Store assets are identified as a `publisher_slug/asset_slug` pair, and each asset can have many releases, so an add always resolves and pins exactly one version.

**Reference (`publisher_slug/asset_slug`):** a bare reference (no version) resolves the **latest stable release compatible with the project's Godot version**. This is the recommended form:

```bash
ggg add asset-store souleat/godot-xoshiro256-plus-plus
```

The exact release that gets pinned is printed on the `Found:` line and stored in `ggg.toml`. If the add runs non-interactively (`--yes`), the pinned release is still resolved at add time — the version string is always written into the config.

**Pinning a version:** append `:version` to the reference:

```bash
ggg add asset-store souleat/godot-xoshiro256-plus-plus:1.1.0
```

Pinning a release that is incompatible with the project's Godot version is allowed but prints a warning; the dependency is still added.

**Query:** anything that is not a `publisher_slug/asset_slug` reference (and not a number) is treated as a store keyword search, filtered to your Godot version, using the standard disambiguation flow.

**`--name`:** override the dependency name instead of accepting the suggested one (derived from the asset's display name).

**`--yes`:** skip prompts. Requires a reference argument — you can't search non-interactively.

### Example output

```
$ ggg add asset-store souleat/godot-xoshiro256-plus-plus
Found: GodotXoshiro256++ (v1.1.0)
  Author:  souleat
  License: MIT
  Godot:   v4.0..latest
  Browse:  https://store.godotengine.org/assets/souleat/godot-xoshiro256-plus-plus

? Name · godotxoshiro256
Added "godotxoshiro256" (souleat/godot-xoshiro256-plus-plus v1.1.0). Run `ggg sync` to install.
```

The resulting entry in `ggg.toml`:

```toml
[[dependency]]
name               = "godotxoshiro256"
asset_store_asset  = "souleat/godot-xoshiro256-plus-plus:1.1.0"
```

Asset store dependencies pin a version, so the latest compatible release only changes when you edit `ggg.toml` and re-add or re-resolve.

### Searching the store

A search term routes to the store under the `asset-store` keyword. Bare plain queries search the store **and** the Asset Library together. In both cases the standard disambiguation flow decides what gets added (see below).

## How search results are disambiguated

The `asset-library` query, `asset-store` query, and bare combined query all behave identically:

- **0 results:** the command fails with `no assets found for "<query>" on Godot <major.minor>`.
- **1 result:** it is selected automatically. You are shown the `Found:` summary and prompted for a name (skipped with `--name` or `--yes`).
- **2–5 results:** an interactive picker lists the matches, each prefixed with its source (`[store]` / `[library]`), with a `Cancel` option.
- **6+ results:** the command fails, suggesting `ggg search <query>` to browse, followed by the exact add invocation to use:
  - store: `ggg add asset-store <publisher>/<slug>`
  - asset-library: `ggg add asset-library --id <N>`

## Notes

- Fails if a dependency with the same name already exists in `ggg.toml`.
- A pure number is always treated as an Asset Library ID. If you pass one to the `asset-store` keyword, ggg errors and points you at `ggg add asset-library --id <N>`.
- `--id` is only valid with `ggg add asset-library`; `--sha256` only with `ggg add archive`.
- `map` and `strip_components` can be set after adding by editing `ggg.toml` directly. Use [`ggg ls-dep`](@/docs/reference/commands/ls-dep.md) to inspect the source tree and determine the right values before running `ggg sync`.
- `ggg add` only modifies `ggg.toml`. Run `ggg sync` to actually install the dependency.

## See also

- [`ggg sync`](@/docs/reference/commands/sync.md): install dependencies after adding
- [`ggg remove`](@/docs/reference/commands/remove.md): remove a dependency
- [`ggg search`](@/docs/reference/commands/search.md): browse the asset library or asset store without adding
- [`ggg update`](@/docs/reference/commands/update.md): check for newer versions of asset library dependencies
- [`ggg ls-dep`](@/docs/reference/commands/ls-dep.md): inspect a dependency's source tree