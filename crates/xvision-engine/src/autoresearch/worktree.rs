//! Worktree lifecycle management for autoresearch training runs.
//!
//! Each run gets an isolated git worktree at `.worktrees/autoresearch-{run_tag}/`
//! on branch `autoresearch/{run_tag}`. All training commits go there; the main
//! checkout (enforced by `.githooks/pre-commit`) is never touched.

use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};

/// A live git worktree for one autoresearch run.
///
/// Created by [`WorktreeHandle::create`] and torn down by [`WorktreeHandle::remove`].
/// The worktree lives at `<checkout_root>/.worktrees/autoresearch-{run_tag}/`
/// on branch `autoresearch/{run_tag}`. Training commits never touch the main
/// checkout (enforced by the pre-commit hook and by this struct's design —
/// callers always get a path under `.worktrees/`).
#[derive(Debug)]
pub struct WorktreeHandle {
    /// Path to the worktree dir, relative to the (possibly non-canonical) repo
    /// root passed by the caller. Keeps `starts_with(repo)` true on all OSes.
    path: PathBuf,
    /// Canonical checkout root used for git command invocations.
    repo_canonical: PathBuf,
    /// `autoresearch/{run_tag}`
    branch: String,
}

/// Validates that `run_tag` is safe for use as a git branch suffix and
/// filesystem path component. Delegates to the canonical pure-Rust checker in
/// `crate::nanochat::validate` (same crate, no regex dependency), then adds
/// path-traversal guards specific to the worktree path construction.
///
/// NOTE (FIX A): Never use `regex::Regex` here — call the canonical
/// `crate::nanochat::validate::validate_run_tag` instead so the rule is
/// defined in exactly one place.
fn validate_run_tag(run_tag: &str) -> Result<()> {
    // Reject any path-separator or traversal component regardless of charset.
    if run_tag.contains('/') || run_tag.contains('\\') || run_tag.contains("..") {
        bail!("invalid run_tag: path traversal detected in {run_tag:?}");
    }
    // Delegate charset + length check to the canonical pure-Rust validator
    // (Task 2.1, crates/xvision-engine/src/nanochat/validate.rs).
    crate::nanochat::validate::validate_run_tag(run_tag)
        .map_err(|msg| anyhow::anyhow!("{msg}"))
}

impl WorktreeHandle {
    /// Create the worktree and branch in `repo`. Validates `run_tag` first, then
    /// calls `git worktree add -b autoresearch/{run_tag}
    /// .worktrees/autoresearch-{run_tag}` from the checkout root.
    ///
    /// The caller must hold onto the returned handle and call [`remove`] when
    /// the run ends. On drop the handle logs a warning but does NOT remove the
    /// worktree automatically (removal requires `git worktree remove` which can
    /// fail — callers must handle cleanup explicitly).
    pub fn create(repo: &Path, run_tag: &str) -> Result<Self> {
        validate_run_tag(run_tag)?;

        // Canonicalize for git commands (handles macOS /var → /private/var symlinks).
        let repo_canonical = repo.canonicalize()
            .with_context(|| format!("canonicalize repo root {}", repo.display()))?;

        let wt_rel = format!(".worktrees/autoresearch-{run_tag}");
        let branch = format!("autoresearch/{run_tag}");

        // Ensure .worktrees/ exists so git doesn't complain about missing parent.
        std::fs::create_dir_all(repo_canonical.join(".worktrees"))
            .context("create .worktrees/ directory")?;

        let status = std::process::Command::new("git")
            .args(["worktree", "add", "-b", &branch, &wt_rel])
            .current_dir(&repo_canonical)
            .status()
            .context("spawn git worktree add")?;

        if !status.success() {
            bail!(
                "git worktree add failed for run_tag={run_tag:?} (exit {:?})",
                status.code()
            );
        }

        // Build path from the caller-provided (possibly non-canonical) repo so
        // that `path.starts_with(repo)` holds even on macOS where TempDir paths
        // are symlinks (/var/… vs /private/var/…).
        let path = repo.join(".worktrees").join(format!("autoresearch-{run_tag}"));

        // Safety check via canonical forms: the resolved path must stay within
        // the canonical checkout root.
        let path_canonical = path.canonicalize()
            .with_context(|| format!("canonicalize worktree path {}", path.display()))?;
        if !path_canonical.starts_with(&repo_canonical) {
            // Remove the just-created worktree before returning an error.
            let _ = std::process::Command::new("git")
                .args(["worktree", "remove", "--force",
                       path_canonical.to_str().unwrap_or("")])
                .current_dir(&repo_canonical)
                .status();
            bail!(
                "worktree path {} escapes checkout root {} — path traversal rejected",
                path_canonical.display(), repo_canonical.display()
            );
        }

        Ok(Self { path, repo_canonical, branch })
    }

    /// The filesystem path to this worktree (built from the caller-supplied repo
    /// root, so `path().starts_with(repo)` is guaranteed).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The git branch name (`autoresearch/{run_tag}`).
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Remove the worktree and delete its branch. Idempotent — if the directory
    /// is already gone, `git worktree prune` cleans up the metadata.
    pub fn remove(self) -> Result<()> {
        // Best-effort remove; prune cleans metadata even if the dir is missing.
        let _ = std::process::Command::new("git")
            .args(["worktree", "remove", "--force",
                   self.path.to_str().unwrap_or("")])
            .current_dir(&self.repo_canonical)
            .status();

        let _ = std::process::Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.repo_canonical)
            .status();

        // Delete the branch so re-running the same run_tag is possible.
        let _ = std::process::Command::new("git")
            .args(["branch", "-D", &self.branch])
            .current_dir(&self.repo_canonical)
            .status();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Bootstrap a bare git repo in `dir` with a single commit on main,
    /// returning the path. Mirrors the real checkout structure so
    /// `git worktree add` succeeds.
    fn init_git_repo(dir: &std::path::Path) {
        fn git(dir: &std::path::Path, args: &[&str]) {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .status()
                .unwrap();
            assert!(status.success(), "git {:?} failed", args);
        }
        git(dir, &["init", "-b", "main"]);
        git(dir, &["config", "user.email", "test@test.com"]);
        git(dir, &["config", "user.name", "Test"]);
        // Need at least one commit so `git worktree add` has a HEAD to fork from.
        std::fs::write(dir.join(".gitkeep"), b"").unwrap();
        git(dir, &["add", ".gitkeep"]);
        git(dir, &["commit", "-m", "init"]);
    }

    #[test]
    fn create_and_remove_worktree_within_checkout_root() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().to_path_buf();
        init_git_repo(&repo);

        let run_tag = "jun12a";
        let wt = WorktreeHandle::create(&repo, run_tag).expect("create worktree");

        // Worktree must land inside the checkout root.
        assert!(wt.path().starts_with(&repo));
        // Must be the canonical path we expect.
        assert_eq!(wt.path(), repo.join(".worktrees").join(format!("autoresearch-{run_tag}")));
        // Branch name is correct.
        assert_eq!(wt.branch(), format!("autoresearch/{run_tag}"));
        // Directory physically exists.
        assert!(wt.path().exists());

        // Remove — directory must be gone.
        wt.remove().expect("remove worktree");
        assert!(!repo.join(".worktrees").join(format!("autoresearch-{run_tag}")).exists());
    }

    #[test]
    fn rejects_run_tag_that_escapes_checkout_root() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().to_path_buf();
        init_git_repo(&repo);

        // A crafted run_tag with `..` that would escape the root.
        let err = WorktreeHandle::create(&repo, "../../etc").unwrap_err();
        assert!(
            err.to_string().contains("invalid run_tag") || err.to_string().contains("traversal"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn create_does_not_touch_main_checkout_head() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().to_path_buf();
        init_git_repo(&repo);

        let head_before = read_head(&repo);
        let wt = WorktreeHandle::create(&repo, "testrun").unwrap();
        let head_after = read_head(&repo);
        assert_eq!(head_before, head_after, "main checkout HEAD was modified");
        wt.remove().unwrap();
    }

    fn read_head(repo: &std::path::Path) -> String {
        std::fs::read_to_string(repo.join(".git").join("HEAD")).unwrap()
    }
}
