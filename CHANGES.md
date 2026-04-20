# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-04-20

### Added
- It is now possible to specify a custom name for dependencies when adding them with a `--name` flag rather than having to do it interactively.

### Fixed

- Archive and asset library dependencies are no longer re-downloaded on every `ggg sync` run when they already present in the cache.

### Changed

- Internal restructuring of the dependency pipeline with no user-visible
  behaviour changes.

## [0.1.0] - 2026-04-14

Initial release.

[Unreleased]: https://github.com/godotneers/ggg/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/godotneers/ggg/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/godotneers/ggg/releases/tag/v0.1.0
