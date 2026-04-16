# Godot Goodie Grabber

**Godot Goodie Grabber** (`ggg`) is a project manager for [Godot](https://godotengine.org/) games, inspired by [uv](https://github.com/astral-sh/uv) for Python.

Declare your Godot version and addon dependencies in a `ggg.toml` file. Then a single command sets up the full project environment - right engine version, all addons installed, every time.

```sh
ggg sync   # downloads Godot, installs all addons
ggg edit   # opens the project in the pinned editor
```

Addons can come from git repositories, pre-built archives, or the [Godot Asset Library](https://godotengine.org/asset-library/). A `ggg.lock` file records exact resolved versions so every contributor and every CI run gets a bit-for-bit identical environment.

## Documentation

**[godotneers.github.io/ggg](https://godotneers.github.io/ggg)**

## License

MIT OR Apache-2.0
