+++
title = "ggg add"
weight = 2
+++

```
ggg add git <url>[@<rev>] [--yes]
ggg add archive <url> [--name <name>] [--strip-components <n>] [--sha256 <hash>]
ggg add <url>
```

Adds a new dependency to `ggg.toml`. Run this from your project directory whenever you want to include a new addon. After adding, run [`ggg sync`](@/docs/reference/commands/sync.md) to install it.

There are two kinds of dependency: **git** (sourced from a repository) and **archive** (a pre-built `.zip` or `.tar.gz` downloaded from a URL). Most Godot addons are available as git repositories, so `ggg add git` is the common case. Use `ggg add archive` for addons distributed as release assets without a public git repository.

If you pass a URL directly without a subcommand, `ggg add` will detect the type from the URL and route accordingly.

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

## Notes

- Fails if a dependency with the same name already exists in `ggg.toml`.
- `map` and `strip_components` can be set after adding by editing `ggg.toml` directly. Use [`ggg ls-dep`](@/docs/reference/commands/ls-dep.md) to inspect the source tree and determine the right values before running `ggg sync`.
- `ggg add` only modifies `ggg.toml`. Run `ggg sync` to actually install the dependency.
