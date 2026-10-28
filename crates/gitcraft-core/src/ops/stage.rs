//! Staging and unstaging whole files. Only the index changes: the working
//! tree is never touched, so no uncommitted work can be lost.

use std::path::Path;

use crate::{CancellationToken, Git, GitCommand, GitError, Result};

/// Paths come from the UI: refuse empty ones and never let git interpret
/// them as globs or pathspec magic.
fn literal_paths(cmd: GitCommand, paths: &[String]) -> Result<GitCommand> {
    if paths.iter().any(|p| p.is_empty() || p.contains('\0')) {
        return Err(GitError::Parse("invalid path".into()));
    }
    Ok(cmd.env("GIT_LITERAL_PATHSPECS", "1").arg("--").args(paths))
}

/// Whether HEAD points to a commit (false before the first commit).
async fn has_head(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<bool> {
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "HEAD^{commit}"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    Ok(out.exit_code == Some(0))
}

/// Stages the given paths: modifications, deletions and new files.
pub async fn stage(
    git: &Git,
    repo: &Path,
    paths: &[String],
    cancel: &CancellationToken,
) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    literal_paths(git.write(repo).args(["add", "--all"]), paths)?
        .output(cancel)
        .await?;
    Ok(())
}

/// Stages every change in the working tree (respecting .gitignore).
pub async fn stage_all(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<()> {
    git.write(repo)
        .args(["add", "--all"])
        .output(cancel)
        .await?;
    Ok(())
}

/// Removes the given paths from the index, back to their HEAD version (or
/// untracked, before the first commit). For a staged rename pass both the
/// old and the new path.
pub async fn unstage(
    git: &Git,
    repo: &Path,
    paths: &[String],
    cancel: &CancellationToken,
) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let cmd = if has_head(git, repo, cancel).await? {
        git.write(repo).args(["restore", "--staged"])
    } else {
        // Nothing to restore from yet: just drop the entries from the index.
        git.write(repo)
            .args(["rm", "--cached", "-r", "--quiet", "--ignore-unmatch"])
    };
    literal_paths(cmd, paths)?.output(cancel).await?;
    Ok(())
}

/// Unstages everything. Unlike `git reset`, keeps an in-progress merge.
pub async fn unstage_all(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<()> {
    unstage(git, repo, &[".".to_owned()], cancel).await
}
