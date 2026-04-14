//! Implementation of `ggg add`.
//!
//! Two subcommands:
//!
//! - `ggg add git <url>[@rev]` - adds a git dependency, resolves the rev
//!   against the remote before writing.
//! - `ggg add archive <url>` - adds an archive dependency; no network call at
//!   add time (the sha256 field, if provided, is verified on first sync).
//!
//! The bare `ggg add <url>` form is kept as a convenience: if the URL ends
//! with a known archive extension (`.zip`, `.tar.gz`, `.tgz`) it routes to
//! the archive path; otherwise it routes to git.  If the type cannot be
//! inferred the user is prompted.

use std::path::Path;

use anyhow::{bail, Context, Result};
use dialoguer::{Input, Select, theme::ColorfulTheme};
use indicatif::ProgressBar;

use crate::config::{Config, Dependency};
use crate::dependency::resolver;

pub fn run_git(git_url: Option<&str>, yes: bool) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;
    let theme = ColorfulTheme::default();

    let (url_arg, rev_arg) = match git_url {
        Some(s) => {
            let (url, rev) = parse_url_rev(s);
            (Some(url), rev)
        }
        None => (None, None),
    };

    if yes && url_arg.is_none() {
        bail!("--yes requires a git URL argument");
    }

    let git = match url_arg {
        Some(u) => u,
        None => Input::with_theme(&theme)
            .with_prompt("Git URL")
            .interact_text()?,
    };

    if yes && rev_arg.is_none() {
        bail!("--yes requires a revision; append it to the URL with @rev");
    }

    let rev = match rev_arg {
        Some(r) => r,
        None => Input::with_theme(&theme)
            .with_prompt("Revision (branch, tag, or commit SHA)")
            .default("main".to_owned())
            .interact_text()?,
    };

    let default_name = infer_name_from_git(&git);
    let name = if yes {
        default_name
    } else {
        Input::with_theme(&theme)
            .with_prompt("Name")
            .default(default_name)
            .interact_text()?
    };

    if config.dependency.iter().any(|d| d.name == name) {
        bail!("a dependency named {:?} already exists in ggg.toml", name);
    }

    let dep = Dependency::new_git(&name, &git, &rev);
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

pub fn run_archive(archive_url: Option<&str>, name_arg: Option<&str>, strip_components: Option<u32>, sha256: Option<&str>) -> Result<()> {
    let ggg_toml = Path::new("ggg.toml");
    let mut config = Config::load(ggg_toml)?;
    let theme = ColorfulTheme::default();

    let url = match archive_url {
        Some(u) => u.to_owned(),
        None => Input::with_theme(&theme)
            .with_prompt("Archive URL (.zip, .tar.gz, .tgz)")
            .interact_text()?,
    };

    // Validate the URL extension up front.
    let supported = url.ends_with(".zip") || url.ends_with(".tar.gz") || url.ends_with(".tgz");
    if !supported {
        bail!("unrecognised archive format in URL {url:?}; supported: .zip, .tar.gz, .tgz");
    }

    let name = match name_arg {
        Some(n) => n.to_owned(),
        None => Input::with_theme(&theme)
            .with_prompt("Name")
            .interact_text()?,
    };

    if name.is_empty() {
        bail!("dependency name cannot be empty");
    }

    if config.dependency.iter().any(|d| d.name == name) {
        bail!("a dependency named {:?} already exists in ggg.toml", name);
    }

    let mut dep = Dependency::new_archive(&name, &url);
    dep.sha256 = sha256.map(str::to_owned);
    dep.strip_components = strip_components;

    config.dependency.push(dep);
    config.save(ggg_toml)?;

    println!("Added {name:?} from {url}");
    if sha256.is_none() {
        println!("Tip: add a sha256 = \"<hash>\" field to verify the download integrity.");
    }
    println!("Run `ggg sync` to install it.");
    Ok(())
}

/// Handle `ggg add <url>` with no explicit subcommand - detect type from URL
/// extension and prompt if ambiguous.
pub fn run_bare(url: &str, yes: bool) -> Result<()> {
    if url.ends_with(".zip") || url.ends_with(".tar.gz") || url.ends_with(".tgz") {
        run_archive(Some(url), None, None, None)
    } else if url.contains("://") || url.ends_with(".git") || url.contains(':') {
        run_git(Some(url), yes)
    } else if yes {
        bail!("cannot determine whether {url:?} is a git URL or an archive; use `ggg add git` or `ggg add archive` explicitly");
    } else {
        // Ask the user.
        let theme = ColorfulTheme::default();
        let choice = Select::with_theme(&theme)
            .with_prompt("Is this a git repository or an archive?")
            .items(&["git repository", "archive (.zip / .tar.gz)"])
            .default(0)
            .interact()?;
        match choice {
            0 => run_git(Some(url), false),
            _ => run_archive(Some(url), None, None, None),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Split `s` into a git URL and an optional revision.
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
fn infer_name_from_git(url: &str) -> String {
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
    fn infer_name_strips_git_suffix_and_lowercases() {
        assert_eq!(infer_name_from_git("https://github.com/bitwes/Gut.git"), "gut");
    }

    #[test]
    fn infer_name_no_git_suffix() {
        assert_eq!(infer_name_from_git("https://github.com/user/my-addon"), "my-addon");
    }

    #[test]
    fn infer_name_trailing_slash() {
        assert_eq!(infer_name_from_git("https://github.com/user/Repo.git/"), "repo");
    }

    #[test]
    fn infer_name_ssh_scp_url() {
        assert_eq!(infer_name_from_git("git@github.com:user/phantom-camera.git"), "phantom-camera");
    }
}
