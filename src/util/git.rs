//! Tiny git helpers — shells out to `git`. We deliberately don't depend
//! on `git2` (too heavy for the one or two checks we need).

use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Context, Result};

/// State of a path in git relative to its working tree.
///
/// `NotARepo` is *not* the same as `Clean` — callers that gate
/// destructive actions on "clean working tree" must treat
/// `NotARepo` as "unknown" and refuse to proceed without explicit
/// `--force`. The previous version of this helper conflated the two
/// (returned `false` for "no uncommitted changes" on every error
/// path), which made `attach`'s safety gate fail-open and could
/// silently delete `layout.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingTreeState {
    /// `git status --porcelain` returned an empty body for the path.
    Clean,
    /// `git status --porcelain` returned a non-empty body for the path.
    Dirty,
    /// The directory exists but is not inside a git working tree.
    /// Destructive callers must refuse without `--force` because we
    /// can't tell whether the user has committed their changes.
    NotARepo,
}

/// Inspect the working-tree state of `path` (relative to `repo_root`).
///
/// Errors if `git` is not on PATH, or if the `git status` invocation
/// fails for a reason that isn't "not a git repo". Callers handle each
/// case explicitly.
pub fn working_tree_state(repo_root: &Path, path: &str) -> Result<WorkingTreeState> {
    if which::which("git").is_err() {
        return Err(anyhow!(
            "`git` not found on PATH — cannot verify the working tree is clean. \
             Install git, or pass `--force` to skip the check."
        ));
    }

    let output = Command::new("git")
        .args(["status", "--porcelain", "--", path])
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("invoking git status in {}", repo_root.display()))?;

    if output.status.success() {
        if String::from_utf8_lossy(&output.stdout).trim().is_empty() {
            return Ok(WorkingTreeState::Clean);
        }
        return Ok(WorkingTreeState::Dirty);
    }

    // Common case: not a git repo. Git prints "fatal: not a git
    // repository" to stderr in any locale that uses ASCII, but the
    // exact message is locale-dependent — fall back to checking the
    // exit code (128) plus the absence of `.git`.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let not_a_repo = stderr.to_lowercase().contains("not a git repository")
        || (output.status.code() == Some(128) && !repo_root.join(".git").exists());
    if not_a_repo {
        return Ok(WorkingTreeState::NotARepo);
    }

    let code = output
        .status
        .code()
        .map_or("killed by signal".to_string(), |c| c.to_string());
    Err(anyhow!(
        "git status failed (exit {code}): {stderr}"
    ))
}
