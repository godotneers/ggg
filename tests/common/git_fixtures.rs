//! Local git repository fixtures for integration tests.
//!
//! These build real bare repositories on disk using the `git` binary. We
//! assume `git` is available on developer machines that run the test suite
//! (after all, the project itself is checked out via git) - the end-user
//! binary does not depend on git, so this is only a test-time dependency.
//!
//! Repository contents are served to `ggg` through the local `file://`
//! transport, which gix reads natively without needing a network daemon.

use std::path::Path;
use std::process::Command;

use super::FileContent;

/// Builder for [`BareRepo`].
///
/// Collects file entries in memory and runs the underlying git commands only
/// when [`build`](BareRepoBuilder::build) is called.
pub struct BareRepoBuilder {
    files: Vec<(String, FileContent)>,
    submodules: Vec<(String, String)>,
}

impl BareRepoBuilder {
    fn new() -> Self {
        Self {
            files: Vec::new(),
            submodules: Vec::new(),
        }
    }

    /// Add a file at `path` (relative to the repo root, forward or back
    /// slashes) with `contents`, which may be UTF-8 text (`&str`) or raw bytes
    /// (`&[u8]`) via [`FileContent`].
    pub fn with(mut self, path: impl Into<String>, contents: impl Into<FileContent>) -> Self {
        self.files.push((path.into(), contents.into()));
        self
    }

    /// Add a submodule at `path` pointing at `commit_sha`.
    ///
    /// The entry is written straight into the index with mode `160000` via
    /// `git update-index --add --cacheinfo`; `commit_sha` is never validated
    /// against the parent repository, so it may refer to a commit that only
    /// exists in the (absent) submodule repository - exactly how a real
    /// submodule looks from the superproject's tree.
    pub fn with_submodule(
        mut self,
        path: impl Into<String>,
        commit_sha: impl Into<String>,
    ) -> Self {
        self.submodules.push((path.into(), commit_sha.into()));
        self
    }

    /// Create the bare repository containing a single commit whose tree is the
    /// collected files.
    ///
    /// A `refs/heads/main` branch and a lightweight `refs/tags/v1.0.0` tag are
    /// created pointing at the commit.
    ///
    /// # Panics
    ///
    /// Panics if `git` is unavailable or fails, or if a temporary directory
    /// cannot be created. This is a test-only helper, so a hard failure is
    /// appropriate.
    pub fn build(self) -> BareRepo {
        // Author the commit in a throwaway worktree, then clone it bare.
        let worktree = tempfile::tempdir().expect("failed to create worktree temp dir");
        let wt = worktree.path();

        run_git(wt, &["init", "-q"]);
        for (rel, contents) in &self.files {
            let abs = wt.join(rel);
            std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
            std::fs::write(&abs, contents.as_bytes()).unwrap();
        }
        run_git(wt, &["add", "-A"]);
        for (path, sha) in &self.submodules {
            let info = format!("160000,{},{}", sha, path);
            run_git(wt, &["update-index", "--add", "--cacheinfo", &info]);
        }
        run_git(
            wt,
            &[
                "-c",
                "user.name=ggg tests",
                "-c",
                "user.email=ggg@tests.invalid",
                "commit",
                "-q",
                "-m",
                "initial commit",
            ],
        );
        // Branch is named `main` so `rev = "main"` resolves.
        run_git(wt, &["branch", "-M", "main"]);

        let bare_dir = tempfile::tempdir().expect("failed to create bare repo temp dir");
        run_git(
            wt,
            &[
                "clone",
                "-q",
                "--bare",
                ".",
                bare_dir.path().to_str().unwrap(),
            ],
        );

        let sha = run_git(wt, &["rev-parse", "HEAD"]);
        // Tag must be created in the bare repo (or re-tagged after clone).
        run_git(bare_dir.path(), &["tag", "v1.0.0"]);

        BareRepo { bare_dir, sha }
    }
}

/// A bare git repository living in a temporary directory.
///
/// Dropping the value removes the repository from disk.
pub struct BareRepo {
    /// The temporary directory that holds the bare repository.
    bare_dir: tempfile::TempDir,
    /// The 40-character SHA of the initial commit.
    sha: String,
}

impl BareRepo {
    /// Start building a bare repository containing a single commit.
    ///
    /// Use [`with`](BareRepoBuilder::with) to add files and
    /// [`build`](BareRepoBuilder::build) to create the repo.
    pub fn builder() -> BareRepoBuilder {
        BareRepoBuilder::new()
    }

    /// The path to the bare repository on disk.
    pub fn repo_path(&self) -> &Path {
        self.bare_dir.path()
    }

    /// The 40-character commit SHA of the initial commit.
    pub fn sha(&self) -> &str {
        &self.sha
    }

    /// A `file://` URL that `ggg` can use as a git dependency source.
    ///
    /// - Unix:   `file:///tmp/repo.git`
    /// - Windows: `file:///C:/path/to/repo.git`
    pub fn file_url(&self) -> String {
        let mut s = self.bare_dir.path().to_string_lossy().replace('\\', "/");
        if !s.starts_with('/') {
            s.insert(0, '/');
        }
        format!("file://{s}")
    }
}

/// Run `git` in `cwd`, asserting success, and return trimmed stdout.
fn run_git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|_| panic!("failed to spawn `git` (is it on PATH?)"));
    if !output.status.success() {
        panic!(
            "`git {args:?}` failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
