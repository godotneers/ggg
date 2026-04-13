//! Implementation of `ggg init`.
//!
//! Creates a `ggg.toml` in the current directory. If a `project.godot` file
//! is present, the Godot version and mono flag are read from it and used as
//! defaults; otherwise the user is prompted. If no `project.godot` is present,
//! a minimal one is created so the project opens correctly in the editor.
//! Adds `.ggg.state` to `.gitignore`, creating the file if necessary.

use std::path::Path;

use anyhow::{bail, Context, Result};
use dialoguer::{Confirm, FuzzySelect, theme::ColorfulTheme};
use indicatif::ProgressBar;

use crate::config::{Config, Project};
use crate::dependency::state::STATE_FILE;
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
        None
    };

    // Fetch the versions manifest.
    let spinner = ProgressBar::new_spinner();
    spinner.set_message("Fetching Godot versions...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    let all_versions = fetch_versions()?;
    spinner.finish_and_clear();

    let stable: Vec<&GodotRelease> = all_versions
        .iter()
        .filter(|r| r.is_stable() && r.version.major >= 3)
        .collect();
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
    ensure_gitignore_entry(Path::new(".gitignore"), STATE_FILE)?;

    println!("Created ggg.toml ({})", config.project.godot);

    if !project_godot.exists() {
        create_project_godot(project_godot, chosen)?;
        println!("Created project.godot");
    }

    Ok(())
}

/// Write a minimal `project.godot` file for the given Godot release.
///
/// The file contains just enough for Godot to recognise and open the project
/// in the editor. The `config_version` is derived from the major version:
/// 3.x uses version 4, 4.x uses version 5.
fn create_project_godot(path: &Path, release: &GodotRelease) -> Result<()> {
    let config_version = match release.version.major {
        3 => 4,
        _ => 5, // 4.x and any future major
    };

    let mut content = format!("config_version={config_version}\n");
    content.push_str("\n[application]\n\nconfig/name=\"\"\n");

    // Godot 4+ records the engine version in config/features.
    if release.version.major >= 4 {
        content.push_str(&format!(
            "config/features=PackedStringArray(\"{}.{}\")\n",
            release.version.major, release.version.minor,
        ));
    }

    std::fs::write(path, content)
        .with_context(|| format!("failed to create {}", path.display()))
}

/// Ensure `entry` appears in `gitignore_path`, creating the file if needed.
///
/// Does nothing if the entry is already present (exact line match).
pub fn ensure_gitignore_entry(gitignore_path: &Path, entry: &str) -> Result<()> {
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(gitignore_path)
            .with_context(|| format!("failed to read {}", gitignore_path.display()))?;
        // Check if any line exactly matches the entry.
        if content.lines().any(|l| l == entry) {
            return Ok(());
        }
        // Append, ensuring the file ends with a newline before we add ours.
        let needs_newline = !content.is_empty() && !content.ends_with('\n');
        let mut to_append = String::new();
        if needs_newline {
            to_append.push('\n');
        }
        to_append.push_str(entry);
        to_append.push('\n');
        std::fs::OpenOptions::new()
            .append(true)
            .open(gitignore_path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, to_append.as_bytes()))
            .with_context(|| format!("failed to update {}", gitignore_path.display()))?;
    } else {
        std::fs::write(gitignore_path, format!("{entry}\n"))
            .with_context(|| format!("failed to create {}", gitignore_path.display()))?;
    }
    Ok(())
}
