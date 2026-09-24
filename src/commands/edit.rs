//! Implementation of `ggg edit`.
//!
//! Resolves the Godot executable (honouring a `--godot` override or
//! `GGG_GODOT_EXECUTABLE`), then launches it with `--editor .` to open the
//! current project. Any additional arguments are forwarded verbatim to the
//! Godot process.

use anyhow::Result;
use std::path::Path;

use crate::config::Config;
use crate::godot::cache::GodotCache;
use crate::godot::engine;
use crate::godot::export_templates;

pub fn run(
    extra_args: &[String],
    with_export_templates: bool,
    godot: Option<String>,
) -> Result<()> {
    let config = Config::load(Path::new("ggg.toml"))?;
    let cache = GodotCache::from_env()?;
    let executable = engine::resolve(
        &config.project.godot,
        &cache,
        godot.as_deref().map(Path::new),
    )?;

    if with_export_templates || config.project.export_templates {
        export_templates::ensure_export_templates(&config.project.godot)?;
    }

    let mut args = vec!["--editor".to_string(), ".".to_string()];
    args.extend_from_slice(extra_args);

    engine::launch(&executable, &args)?;
    Ok(())
}
