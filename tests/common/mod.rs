#![allow(dead_code)]

pub mod archive;
pub mod git_fixtures;
pub mod wiremock;

use assert_cmd::Command;
use ggg::config::{Config, Dependency, Project, Sync};
use tempfile::TempDir;

/// The content of a fixture file: either UTF-8 text or raw bytes.
///
/// Fixtures (repositories, test-project files) frequently need both, so this
/// enum lets a single API accept either via [`From`] conversions.
#[derive(Clone)]
pub enum FileContent {
    Text(String),
    Bytes(Vec<u8>),
}

impl FileContent {
    /// The file's bytes, regardless of which variant holds them.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            FileContent::Text(s) => s.as_bytes(),
            FileContent::Bytes(b) => b,
        }
    }
}

impl From<&str> for FileContent {
    fn from(s: &str) -> Self {
        FileContent::Text(s.to_string())
    }
}

impl From<String> for FileContent {
    fn from(s: String) -> Self {
        FileContent::Text(s)
    }
}

impl From<&[u8]> for FileContent {
    fn from(b: &[u8]) -> Self {
        FileContent::Bytes(b.to_vec())
    }
}

impl From<Vec<u8>> for FileContent {
    fn from(b: Vec<u8>) -> Self {
        FileContent::Bytes(b)
    }
}

/// An isolated project directory for running `ggg` against.
///
/// Owns temporary directories for the project and the shared `GGG_CACHE_DIR`,
/// so tests are hermetic without having to manage either manually. Provides
/// helpers that automatically target these directories.
pub struct TestProject {
    dir: TempDir,
    cache_dir: TempDir,
    envs: Vec<(String, String)>,
}

impl TestProject {
    pub fn new() -> Self {
        let cache_dir = tempfile::tempdir().unwrap();
        let cache_dir_str = cache_dir.path().to_str().unwrap().to_string();
        let mut project = Self {
            dir: tempfile::tempdir().unwrap(),
            cache_dir,
            envs: Vec::new(),
        };
        // Point every spawned command at the isolated cache by default.
        // Individual tests can still override it via `env` if needed.
        project.env(ggg::envvars::CACHE_DIR_ENV_VAR, cache_dir_str);
        project
    }

    /// Set an environment variable for every command spawned by this project.
    ///
    /// Mostly used for the `GGG_*` overrides (e.g. `GGG_GODOT_MANIFEST_URL`,
    /// `GGG_ASSET_LIB_API_URL`, `GGG_CACHE_DIR`) so tests can point the binary
    /// at a local server or an isolated cache. Any variable name is accepted.
    pub fn env(&mut self, var: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.envs.push((var.into(), value.into()));
        self
    }

    /// The compiled `ggg` binary, run with the project directory as cwd and
    /// any env vars set via [`Self::env`] applied.
    pub fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("ggg").unwrap();
        cmd.current_dir(self.dir.path());
        for (var, value) in &self.envs {
            cmd.env(var, value);
        }
        cmd
    }

    /// Point this project's `GGG_*_URL` endpoint overrides at a running mock
    /// server, so every spawned command talks to the hermetic test network.
    ///
    /// Wires the Godot versions manifest (`GGG_GODOT_MANIFEST_URL`) to
    /// `<base>/manifest` and the Asset Library API (`GGG_ASSET_LIB_API_URL`)
    /// to `<base>`, matching the stubs mounted by
    /// [`wiremock::MockApi::mount_manifest`], [`wiremock::MockApi::mount_asset_search`]
    /// and [`wiremock::MockApi::mount_asset_detail`]. Endpoints without a
    /// mounted stub are simply never reached.
    pub fn env_api(&mut self, api: &wiremock::MockApi) -> &mut Self {
        self.env(
            ggg::envvars::GODOT_MANIFEST_URL_ENV_VAR,
            format!("{}/manifest", api.base_url()),
        );
        self.env(ggg::envvars::ASSET_LIB_API_URL_ENV_VAR, api.base_url());
        self
    }

    /// Point the Godot builds API (`GGG_GODOT_BUILDS_API_URL`) at a running
    /// mock server so engine downloads round-trip through the hermetic
    /// network (matching [`wiremock::MockApi::mount_builds_api`] and
    /// [`wiremock::MockApi::mount_godot_release`]).
    pub fn env_builds_api(&mut self, api: &wiremock::MockApi) -> &mut Self {
        self.env(
            ggg::envvars::GODOT_BUILDS_API_URL_ENV_VAR,
            format!("{}/releases/tags", api.base_url()),
        );
        self
    }

    /// Point the Godot downloads base (`GGG_GODOT_DOWNLOADS_BASE_URL`) at a
    /// running mock server so export-template downloads round-trip through
    /// the hermetic network (matching
    /// [`wiremock::MockApi::mount_export_templates`]).
    pub fn env_downloads_base(&mut self, api: &wiremock::MockApi) -> &mut Self {
        self.env(
            ggg::envvars::GODOT_DOWNLOADS_BASE_URL_ENV_VAR,
            api.base_url(),
        );
        self
    }

    /// Override the Godot data directory (`GGG_GODOT_DATA_DIR`) so export
    /// templates install into an isolated directory instead of the real
    /// platform data dir.
    pub fn env_data_dir(&mut self, dir: &std::path::Path) -> &mut Self {
        self.env(
            ggg::envvars::GODOT_DATA_DIR_ENV_VAR,
            dir.display().to_string(),
        );
        self
    }

    /// Start a fluent builder for a `ggg.toml` fixture in this project.
    ///
    /// If `ggg.toml` already exists (e.g. from a prior `write()` or from
    /// `ggg init`), the builder is pre-populated from it. Dependency methods
    /// replace by name; `.write()` replaces the full config.
    pub fn config(&self) -> ConfigBuilder<'_> {
        ConfigBuilder::new(self)
    }

    /// Read the contents of a project-relative file as UTF-8.
    ///
    /// Panics if the file does not exist or is not valid UTF-8.
    pub fn read(&self, rel: &str) -> String {
        let path = self.dir.path().join(rel);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
    }

    /// Write `contents` to a project-relative path, creating parent
    /// directories as needed.
    ///
    /// `contents` may be UTF-8 text (`&str`/`String`) or raw bytes
    /// (`&[u8]`/`Vec<u8>`) via [`FileContent`]. Intended for simulating user
    /// modifications (e.g. editing an installed file) in conflict/force tests.
    pub fn write(&self, rel: &str, contents: impl Into<FileContent>) -> &Self {
        let path = self.dir.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents.into().as_bytes()).unwrap();
        self
    }

    /// Whether a project-relative path exists.
    pub fn exists(&self, rel: &str) -> bool {
        self.dir.path().join(rel).exists()
    }

    /// The isolated cache directory this project points `GGG_CACHE_DIR` at.
    ///
    /// Download/extraction side effects of running `ggg` (engine binaries,
    /// dependency archives) land here, so tests can assert on them.
    pub fn cache_dir(&self) -> &std::path::Path {
        self.cache_dir.path()
    }

    /// Load the project's `ggg.toml` through the library API.
    pub fn read_ggg_toml(&self) -> Config {
        Config::load(&self.dir.path().join("ggg.toml")).unwrap()
    }
}

/// A fluent builder for `ggg.toml` fixtures.
///
/// The builder is pre-populated from the existing `ggg.toml` (if any) so that
/// successive `.write()` calls update rather than lose state. Dependency
/// methods replace by name; `.write()` replaces the full config file and
/// re-seeds the Godot cache.
pub struct ConfigBuilder<'p> {
    project: &'p TestProject,
    godot: String,
    export_templates: bool,
    sync: Option<Sync>,
    deps: Vec<Dependency>,
}

impl<'p> ConfigBuilder<'p> {
    fn new(project: &'p TestProject) -> Self {
        let toml_path = project.dir.path().join("ggg.toml");
        if toml_path.exists() {
            let existing = Config::load(&toml_path).unwrap();
            Self {
                project,
                godot: existing.project.godot.to_string(),
                export_templates: existing.project.export_templates,
                sync: existing.sync,
                deps: existing.dependency,
            }
        } else {
            Self {
                project,
                godot: "4.3-stable".to_string(),
                export_templates: false,
                sync: None,
                deps: Vec::new(),
            }
        }
    }

    /// Override the pinned Godot release.
    pub fn godot(mut self, release: impl Into<String>) -> Self {
        self.godot = release.into();
        self
    }

    /// Set whether sync should install export templates for the pinned
    /// release (writes `export_templates = true` in the `[project]` table).
    pub fn export_templates(mut self, enabled: bool) -> Self {
        self.export_templates = enabled;
        self
    }

    /// Set `force_overwrite` glob patterns for the `[sync]` table.
    ///
    /// Use this to bypass conflict detection for files matching these patterns
    /// (e.g. `**/*.import` for engine-generated metadata).
    pub fn sync_force_overwrite(mut self, patterns: &[&str]) -> Self {
        self.sync = Some(Sync {
            force_overwrite: patterns.iter().map(|s| s.to_string()).collect(),
        });
        self
    }

    /// Add (or update) a git-sourced dependency.
    ///
    /// If a dependency with the same name already exists, it is replaced in
    /// place.
    pub fn git(
        mut self,
        name: impl Into<String>,
        git: impl Into<String>,
        rev: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let new = Dependency::new_git(&name, git, rev);
        match self.deps.iter_mut().find(|d| d.name == name) {
            Some(existing) => *existing = new,
            None => self.deps.push(new),
        }
        self
    }

    /// Add (or update) an archive-sourced dependency.
    ///
    /// If a dependency with the same name already exists, it is replaced in
    /// place.
    pub fn archive(mut self, name: impl Into<String>, url: impl Into<String>) -> Self {
        let name = name.into();
        let new = Dependency::new_archive(&name, url);
        match self.deps.iter_mut().find(|d| d.name == name) {
            Some(existing) => *existing = new,
            None => self.deps.push(new),
        }
        self
    }

    /// Add (or update) an Asset Library dependency by numeric asset ID.
    ///
    /// If a dependency with the same name already exists, it is replaced in
    /// place.
    pub fn asset(mut self, name: impl Into<String>, asset_id: u32) -> Self {
        let name = name.into();
        let new = Dependency::new_asset_lib(&name, asset_id);
        match self.deps.iter_mut().find(|d| d.name == name) {
            Some(existing) => *existing = new,
            None => self.deps.push(new),
        }
        self
    }

    /// Write the `ggg.toml` into the project directory and seed the Godot
    /// cache for the pinned release so `ggg sync` runs offline.
    pub fn write(self) {
        self.write_inner(true)
    }

    /// Write the `ggg.toml` into the project directory WITHOUT seeding the
    /// Godot cache.
    ///
    /// Use this to exercise the real engine-download path: with an empty
    /// cache, `ggg sync` must query the (mocked) builds API and install the
    /// engine rather than finding a pre-seeded dummy binary.
    pub fn write_no_seed(self) {
        self.write_inner(false)
    }

    /// Shared implementation of [`Self::write`] and [`Self::write_no_seed`]:
    /// writes `ggg.toml`, then seeds the Godot cache when `seed` is `true`.
    fn write_inner(mut self, seed: bool) {
        let config = Config {
            project: Project {
                godot: self.godot.parse().unwrap(),
                export_templates: self.export_templates,
            },
            sync: self.sync.take(),
            dependency: std::mem::take(&mut self.deps),
        };
        config
            .save(&self.project.dir.path().join("ggg.toml"))
            .unwrap();
        if seed {
            self.seed_godot(&config.project.godot.to_string());
        }
    }

    /// Seed the Godot engine cache so `ggg sync` believes `release` is
    /// installed without downloading anything.
    ///
    /// `ggg sync` calls `engine::ensure`, which only checks that the release's
    /// cache directory exists and contains at least one file matching the
    /// executable name pattern - it never launches the binary. A 0-byte dummy
    /// file therefore satisfies the check and keeps the test offline.
    fn seed_godot(&self, release: &str) {
        let dir = self.project.cache_dir.path().join("godot").join(release);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = if cfg!(target_os = "windows") {
            format!("Godot_{release}.exe")
        } else {
            format!("godot_{release}_linux")
        };
        std::fs::write(dir.join(exe), b"").unwrap();
    }
}
