use anyhow::Result;

use crate::config::Config;
use crate::godot::cache::GodotCache;
use crate::godot::engine;

pub fn run(args: &[String]) -> Result<()> {
    let config = Config::load(std::path::Path::new("ggg.toml"))?;
    let cache = GodotCache::from_env()?;
    let executable = engine::ensure(&config.project.godot, &cache)?;
    engine::launch(&executable, args)?;
    Ok(())
}
