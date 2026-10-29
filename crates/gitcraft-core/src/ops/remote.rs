//! Talking to remotes: fetch, pull and push.
//!
//! Authentication is left to the user's credential helper or SSH agent;
//! GitCRAFT never stores or asks for credentials, and every command runs
//! with `GIT_TERMINAL_PROMPT=0`, so a missing credential fails fast with an
//! `authFailed` error instead of waiting for a prompt nobody can answer.
//! Progress comes from git's own `--progress` lines and every operation can
//! be cancelled.

use std::path::Path;

use crate::{CancellationToken, Git, GitError, Result};

/// Configured remotes (`git remote`).
pub async fn remotes(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<Vec<String>> {
    let out = git.read(repo).arg("remote").output(cancel).await?;
    Ok(crate::parse::refs::parse_remotes(&out.stdout_lossy()))
}

/// Remote names reach git's command line only if they are configured.
async fn known_remote(
    git: &Git,
    repo: &Path,
    remote: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    if remote.starts_with('-')
        || !remotes(git, repo, cancel)
            .await?
            .iter()
            .any(|r| r == remote)
    {
        return Err(GitError::Parse(format!("unknown remote {remote:?}")));
    }
    Ok(())
}

/// Downloads new commits and refs from `remote` (all remotes when `None`),
/// removing remote branches deleted there when `prune`. Never touches local
/// branches or the working tree.
pub async fn fetch(
    git: &Git,
    repo: &Path,
    remote: Option<&str>,
    prune: bool,
    cancel: &CancellationToken,
    on_progress: impl FnMut(&str),
) -> Result<()> {
    let mut cmd = git
        .write(repo)
        .args(["fetch", "--progress"])
        // Report progress from the start instead of after git's 2 s delay.
        .env("GIT_PROGRESS_DELAY", "0");
    if prune {
        cmd = cmd.arg("--prune");
    }
    match remote {
        Some(remote) => {
            known_remote(git, repo, remote, cancel).await?;
            cmd = cmd.arg(remote);
        }
        None => cmd = cmd.arg("--all"),
    }
    cmd.output_with_progress(cancel, on_progress).await?;
    Ok(())
}
