//! Implementation of `ggg add`.
//!
//! Appends a new `[[dependency]]` entry to `ggg.toml`. The git URL and
//! revision can be supplied on the command line (`url@rev`) or entered
//! interactively. The revision is resolved against the remote before writing
//! so typos are caught immediately. Run `ggg sync` afterwards to install.

use std::path::Path;

use anyhow::{bail, Context, Result};
use dialoguer::{Input, theme::ColorfulTheme};
use indicatif::ProgressBar;

use crate::config::{Config, Dependency};
use crate::dependency::resolver;

pub fn run(git_url: Option<&str>, yes: bool) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;

    let theme = ColorfulTheme::default();

    // Split the positional argument into URL and optional @rev.
    let (url_arg, rev_arg) = match git_url {
        Some(s) => {
            let (url, rev) = parse_url_rev(s);
            (Some(url), rev)
        }
        None => (None, None),
    };

    // Require URL when --yes is set; there is nothing to infer.
    if yes && url_arg.is_none() {
        bail!("--yes requires a git URL argument");
    }

    // Prompt for URL if not provided.
    let git = match url_arg {
        Some(u) => u,
        None => Input::with_theme(&theme)
            .with_prompt("Git URL")
            .interact_text()?,
    };

    // Require rev when --yes is set and none was given in the URL.
    if yes && rev_arg.is_none() {
        bail!("--yes requires a revision; append it to the URL with @rev");
    }

    // Prompt for revision if not provided.
    let rev = match rev_arg {
        Some(r) => r,
        None => Input::with_theme(&theme)
            .with_prompt("Revision (branch, tag, or commit SHA)")
            .default("main".to_owned())
            .interact_text()?,
    };

    // Infer a default name from the repository slug.
    let default_name = infer_name(&git);

    // Prompt for name, defaulting to the inferred slug.
    let name = if yes {
        default_name
    } else {
        Input::with_theme(&theme)
            .with_prompt("Name")
            .default(default_name)
            .interact_text()?
    };

    // Reject the name if it already exists in ggg.toml.
    if config.dependency.iter().any(|d| d.name == name) {
        bail!("a dependency named {:?} already exists in ggg.toml", name);
    }

    // Resolve the revision against the remote to catch typos early.
    let dep = Dependency { name: name.clone(), git: git.clone(), rev: rev.clone(), map: None };
    let spinner = ProgressBar::new_spinner();
    spinner.set_message(format!("Resolving {rev}..."));
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    let resolved = resolver::resolve(&dep)
        .with_context(|| format!("failed to resolve {:?} from {git:?}", rev))?;
    spinner.finish_and_clear();

    config.dependency.push(dep);
    config.save(ggg_toml)?;

    println!("Added {name:?} ({rev}) resolved to {}", &resolved.sha[..12]);
    println!("Run `ggg sync` to install it.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Split `s` into a git URL and an optional revision.
///
/// The `@rev` suffix is recognised only when the left-hand side looks like a
/// git URL (contains `://`, ends with `.git`, or uses SCP-like `host:path`
/// syntax). This avoids misreading the user@ portion of an SSH URL such as
/// `git@github.com:user/repo.git` as a rev separator.
fn parse_url_rev(s: &str) -> (String, Option<String>) {
    if let Some((left, right)) = s.rsplit_once('@') {
        let looks_like_url = left.contains("://")
            || left.ends_with(".git")
            || left.contains(':');
        if looks_like_url {
            return (left.to_owned(), Some(right.to_owned()));
        }
    }
    (s.to_owned(), None)
}

/// Derive a dependency name from a git URL.
///
/// Takes the last path segment, strips a `.git` suffix, and lowercases it.
/// For example, `https://github.com/bitwes/Gut.git` becomes `"gut"`.
fn infer_name(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(url)
        .trim_end_matches(".git")
        .to_lowercase()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_url_rev -------------------------------------------------------

    #[test]
    fn https_url_with_rev() {
        let (url, rev) = parse_url_rev("https://github.com/bitwes/Gut.git@v9.3.0");
        assert_eq!(url, "https://github.com/bitwes/Gut.git");
        assert_eq!(rev.as_deref(), Some("v9.3.0"));
    }

    #[test]
    fn https_url_without_rev() {
        let (url, rev) = parse_url_rev("https://github.com/bitwes/Gut.git");
        assert_eq!(url, "https://github.com/bitwes/Gut.git");
        assert!(rev.is_none());
    }

    #[test]
    fn ssh_scp_url_with_rev() {
        let (url, rev) = parse_url_rev("git@github.com:user/repo.git@main");
        assert_eq!(url, "git@github.com:user/repo.git");
        assert_eq!(rev.as_deref(), Some("main"));
    }

    #[test]
    fn ssh_scp_url_without_rev() {
        let (url, rev) = parse_url_rev("git@github.com:user/repo.git");
        assert_eq!(url, "git@github.com:user/repo.git");
        assert!(rev.is_none());
    }

    #[test]
    fn https_url_with_branch_rev() {
        let (url, rev) = parse_url_rev("https://github.com/user/repo.git@main");
        assert_eq!(url, "https://github.com/user/repo.git");
        assert_eq!(rev.as_deref(), Some("main"));
    }

    // --- infer_name ----------------------------------------------------------

    #[test]
    fn infer_name_strips_git_suffix_and_lowercases() {
        assert_eq!(infer_name("https://github.com/bitwes/Gut.git"), "gut");
    }

    #[test]
    fn infer_name_no_git_suffix() {
        assert_eq!(infer_name("https://github.com/user/my-addon"), "my-addon");
    }

    #[test]
    fn infer_name_trailing_slash() {
        assert_eq!(infer_name("https://github.com/user/Repo.git/"), "repo");
    }

    #[test]
    fn infer_name_ssh_scp_url() {
        assert_eq!(infer_name("git@github.com:user/phantom-camera.git"), "phantom-camera");
    }
}
