+++
title = "ggg add"
weight = 2
+++

```
ggg add git <url>[@<rev>] [--yes]
ggg add archive <url> [--name <name>] [--strip-components <n>] [--sha256 <hash>]
ggg add asset [<name-or-id>] [--id <N>] [--yes]
ggg add <url-or-name>
```

Adds a new dependency to `ggg.toml`. Run this from your project directory whenever you want to include a new addon. After adding, run [`ggg sync`](@/docs/reference/commands/sync.md) to install it.

There are three kinds of dependency: **git** (sourced from a repository), **archive** (a pre-built `.zip` or `.tar.gz` downloaded from a URL), and **asset** (sourced from the [Godot Asset Library](https://godotengine.org/asset-library/)). Most Godot addons are available as git repositories, so `ggg add git` is the common case. Use `ggg add asset` to install directly from the asset library without having to look up download URLs manually.

If you pass a URL or search term directly without a subcommand, `ggg add` will detect the type and route accordingly.

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

**Name:** a short identifier used in CLI output and as the argument to `ggg remove`. A name is suggested based on the repository name (lowercased, `.git` suffix removed). Press Enter to accept it or type a different one.

### `--yes`

Skips all prompts and uses the suggested name. Requires the URL to include a revision (`url@rev` form); the command fails if the revision is missing.

```bash
ggg add git https://github.com/bitwes/Gut.git@v9.3.0 --yes
```

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

**`--name`:** required. A short identifier for the dependency. You will be prompted if it is not supplied.

**`--sha256`:** the expected SHA-256 hex digest of the archive. Strongly recommended: `ggg sync` will verify every download against this hash and fail if they do not match, protecting against accidental or malicious changes at the source URL. If omitted, a reminder is printed.

**`--strip-components`:** number of leading path components to strip from the archive contents before installing (equivalent to `tar --strip-components`). Useful when the archive wraps everything in a top-level directory. See the [configuration reference](@/docs/reference/configuration.md) for details.

### Example output

```
$ ggg add archive https://example.com/debug_draw_3d.zip --name debug-draw
Added "debug-draw" from https://example.com/debug_draw_3d.zip
Tip: add a sha256 = "<hash>" field to verify the download integrity.
Run `ggg sync` to install it.
```

## `ggg add <url>` (bare form)

Passing a URL directly without a subcommand is a shorthand:

- URLs ending in `.zip`, `.tar.gz`, or `.tgz` are treated as archive dependencies.
- URLs containing `://`, ending in `.git`, or using SSH syntax (`git@...`) are treated as git dependencies.
- If the type cannot be determined, you are prompted to choose.

## `ggg add asset`

```bash
ggg add asset gut
ggg add asset --id 54
```

Searches the [Godot Asset Library](https://godotengine.org/asset-library/) and adds the selected asset as a dependency. The Godot version declared in `ggg.toml` is used to filter results to compatible assets.

**Query:** a name or keyword to search for. If you already know the numeric asset ID, pass it directly as the query or use `--id`.

**`--id`:** bypass the search and add the asset with this ID directly. Useful when you have the asset page URL from the website.

**`--yes`:** accept the suggested dependency name without prompting.

If the search returns a single match it is selected automatically. If there are multiple matches (up to five) you are shown a numbered list and asked to pick one. If there are more than five results a message is printed suggesting you refine your query or use `--id`.

### Example output

```
$ ggg add asset gut
  1. GUT - Godot Unit Testing (v9.3.0) [id=54]
  2. GUT Godot Unit Test - another entry
> 1
? Name · gut
Added "gut" (asset id=54, v9.3.0).
Run `ggg sync` to install it.
```

The resulting entry in `ggg.toml`:

```toml
[[dependency]]
name     = "gut"
asset_id = 54
```

Asset library dependencies default to `strip_components = 1` because the asset library packages every asset inside a top-level folder. Override this in `ggg.toml` if needed.

## `ggg add <url-or-term>` (bare form)

Passing input directly without a subcommand is a shorthand:

- URLs ending in `.zip`, `.tar.gz`, or `.tgz` are treated as archive dependencies.
- URLs containing `://`, ending in `.git`, or using SSH syntax (`git@...`) are treated as git dependencies.
- Anything else is passed to the Godot Asset Library search (same as `ggg add asset <term>`).

## Notes

- Fails if a dependency with the same name already exists in `ggg.toml`.
- `map` and `strip_components` can be set after adding by editing `ggg.toml` directly. Use [`ggg ls-dep`](@/docs/reference/commands/ls-dep.md) to inspect the source tree and determine the right values before running `ggg sync`.
- `ggg add` only modifies `ggg.toml`. Run `ggg sync` to actually install the dependency.

## See also

- [`ggg sync`](@/docs/reference/commands/sync.md): install dependencies after adding
- [`ggg remove`](@/docs/reference/commands/remove.md): remove a dependency
- [`ggg search`](@/docs/reference/commands/search.md): browse the asset library without adding
- [`ggg update`](@/docs/reference/commands/update.md): check for newer versions of asset library dependencies
- [`ggg ls-dep`](@/docs/reference/commands/ls-dep.md): inspect a dependency's source tree
