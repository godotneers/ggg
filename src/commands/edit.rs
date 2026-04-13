//! Implementation of `ggg edit`.
//!
//! Ensures the pinned Godot version is cached, then launches it with
//! `--editor .` to open the current project. Any additional arguments are
//! forwarded verbatim to the Godot process.

use anyhow::Result;

use crate::config::Config;
use crate::godot::cache::GodotCache;
use crate::godot::engine;

pub fn run(extra_args: &[String]) -> Result<()> {
    let config = Config::load(std::path::Path::new("ggg.toml"))?;
    let cache = GodotCache::from_env()?;
    let executable = engine::ensure(&config.project.godot, &cache)?;

    let mut args = vec!["--editor".to_string(), ".".to_string()];
    args.extend_from_slice(extra_args);

    engine::launch(&executable, &args)?;
    Ok(())
}
