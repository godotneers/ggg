use std::path::Path;

use anyhow::{bail, Result};
use dialoguer::{Confirm, FuzzySelect, theme::ColorfulTheme};
use indicatif::ProgressBar;

use crate::config::{Config, Project};
use crate::godot::manifest::fetch_versions;
use crate::godot::project::read_project_info;
use crate::godot::release::GodotRelease;

pub fn run() -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let project_godot = Path::new("project.godot");

    if ggg_toml.exists() {
        bail!("ggg.toml already exists in the current directory");
    }

    // Read project.godot for version and mono hints, if present.
    let project_info = if project_godot.exists() {
        read_project_info(project_godot)?
    } else {
        eprintln!("warning: no project.godot found in the current directory");
        None
    };

    // Fetch the versions manifest.
    let spinner = ProgressBar::new_spinner();
    spinner.set_message("Fetching Godot versions...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    let all_versions = fetch_versions()?;
    spinner.finish_and_clear();

    let stable: Vec<&GodotRelease> = all_versions.iter().filter(|r| r.is_stable()).collect();
    if stable.is_empty() {
        bail!("no stable Godot releases found in the manifest");
    }

    // Pre-select the version that matches the one found in project.godot.
    let default_index = project_info
        .as_ref()
        .and_then(|info| stable.iter().position(|r| r.version == info.version))
        .unwrap_or(0);

    let theme = ColorfulTheme::default();

    let items: Vec<String> = stable.iter().map(|r| r.version.to_string()).collect();
    let selected = FuzzySelect::with_theme(&theme)
        .with_prompt("Godot version")
        .items(&items)
        .default(default_index)
        .interact()?;

    let chosen = stable[selected];

    // Use the mono flag from project.godot if available, otherwise ask.
    let mono = match &project_info {
        Some(info) => info.mono,
        None => Confirm::with_theme(&theme)
            .with_prompt("Use Mono (C#) build?")
            .default(false)
            .interact()?,
    };

    let release = GodotRelease { version: chosen.version.clone(), flavor: chosen.flavor.clone(), mono };

    let config = Config { project: Project { godot: release }, dependency: vec![] };
    config.save(ggg_toml)?;

    println!("Created ggg.toml ({})", config.project.godot);
    Ok(())
}
