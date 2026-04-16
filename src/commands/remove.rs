//! Implementation of `ggg remove`.
//!
//! Removes a named dependency from `ggg.toml`. No files are deleted from the
//! project at this point - run `ggg sync` afterwards and the stale files will
//! be cleaned up.

use std::path::Path;

use anyhow::{bail, Result};

use crate::config::Config;

pub fn run(name: &str) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;

    if !config.has_dependency(name) {
        bail!("no dependency named {:?} found in ggg.toml", name);
    }

    config.remove_dependency(name);
    config.save(ggg_toml)?;

    println!("Removed {name:?} from ggg.toml.");
    println!("Run `ggg sync` to uninstall its files from the project.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, Dependency, Project};
    use std::path::Path;

    fn write_config(path: &Path, deps: &[(&str, &str, &str)]) {
        let config = Config {
            project: Project { godot: "4.3-stable".parse().unwrap() },
            sync: None,
            dependency: deps.iter().map(|(name, git, rev)| {
                Dependency::new_git(*name, *git, *rev)
            }).collect(),
        };
        config.save(path).unwrap();
    }

    fn remove(path: &Path, name: &str) {
        let mut config = Config::load(path).unwrap();
        config.dependency.retain(|d| d.name != name);
        config.save(path).unwrap();
    }

    #[test]
    fn removes_named_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggg.toml");
        write_config(&path, &[
            ("gut",            "https://github.com/bitwes/Gut.git",            "v9.3.0"),
            ("phantom-camera", "https://github.com/ramokz/phantom-camera.git", "main"),
        ]);

        remove(&path, "gut");

        let reloaded = Config::load(&path).unwrap();
        assert_eq!(reloaded.dependency.len(), 1);
        assert_eq!(reloaded.dependency[0].name, "phantom-camera");
    }

    #[test]
    fn removes_only_the_named_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggg.toml");
        write_config(&path, &[
            ("a", "https://github.com/u/a.git", "main"),
            ("b", "https://github.com/u/b.git", "main"),
            ("c", "https://github.com/u/c.git", "main"),
        ]);

        remove(&path, "b");

        let reloaded = Config::load(&path).unwrap();
        assert_eq!(reloaded.dependency.len(), 2);
        assert_eq!(reloaded.dependency[0].name, "a");
        assert_eq!(reloaded.dependency[1].name, "c");
    }

    #[test]
    fn unknown_name_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggg.toml");
        write_config(&path, &[
            ("gut", "https://github.com/bitwes/Gut.git", "v9.3.0"),
        ]);

        let config = Config::load(&path).unwrap();
        assert!(!config.dependency.iter().any(|d| d.name == "nonexistent"));
    }

    #[test]
    fn removing_last_dependency_leaves_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ggg.toml");
        write_config(&path, &[
            ("gut", "https://github.com/bitwes/Gut.git", "v9.3.0"),
        ]);

        remove(&path, "gut");

        let reloaded = Config::load(&path).unwrap();
        assert!(reloaded.dependency.is_empty());
    }
}
