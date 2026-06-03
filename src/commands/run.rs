//! Implementation of `ggg run`.
//!
//! Ensures the pinned Godot version is cached, then launches it against the
//! current project. Any additional arguments are forwarded verbatim to the
//! Godot process.

use anyhow::Result;

use crate::config::Config;
use crate::godot::cache::GodotCache;
use crate::godot::engine;
use crate::godot::export_templates;

pub fn run(args: &[String], with_export_templates: bool) -> Result<()> {
    let config = Config::load(std::path::Path::new("ggg.toml"))?;
    let cache = GodotCache::from_env()?;
    let executable = engine::ensure(&config.project.godot, &cache)?;

    if with_export_templates || config.project.export_templates {
        export_templates::ensure_export_templates(&config.project.godot)?;
    }

    engine::launch(&executable, args)?;
    Ok(())
}
