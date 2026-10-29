//! Creating a new repository in an existing folder (`git init`).

use std::path::Path;

use serde::Serialize;

use crate::{CancellationToken, Git, GitError, Result};

/// What `git init` would do in a folder, shown before doing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InitCheck {
    /// Initial branch suggested: the user's `init.defaultBranch`, or "main".
    pub default_branch: String,
    /// The folder already is the top of a repository (nothing to create).
    pub already_repository: bool,
    /// The folder is inside another repository's working tree: the new one
    /// would be nested in it.
    pub inside: Option<String>,
    /// The folder has no files (hidden ones count).
    pub empty: bool,
}

fn existing_dir(path: &Path) -> Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(GitError::PathNotFound(path.to_path_buf()))
    }
}

pub async fn check_init(git: &Git, path: &Path, cancel: &CancellationToken) -> Result<InitCheck> {
    existing_dir(path)?;
    let toplevel = git
        .read(path)
        .args(["rev-parse", "--show-toplevel"])
        .accept_exit_code(128)
        .output(cancel);
    let default = git
        .read(path)
        .args(["config", "--get", "init.defaultBranch"])
        .accept_exit_code(1)
        .output(cancel);
    let (toplevel, default) = tokio::try_join!(toplevel, default)?;
    let toplevel = toplevel.stdout_lossy().trim().to_owned();
    let canonical = std::fs::canonicalize(path)?;
    let already_repository =
        !toplevel.is_empty() && std::fs::canonicalize(&toplevel).is_ok_and(|top| top == canonical);
    let inside = (!toplevel.is_empty() && !already_repository).then_some(toplevel);
    let default_branch = default.stdout_lossy().trim().to_owned();
    Ok(InitCheck {
        default_branch: if default_branch.is_empty() {
            "main".to_owned()
        } else {
            default_branch
        },
        already_repository,
        inside,
        empty: std::fs::read_dir(path)?.next().is_none(),
    })
}

/// Whether `name` is a valid branch name (no repository needed).
pub async fn valid_branch_name(
    git: &Git,
    cwd: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<bool> {
    let out = git
        .read(cwd)
        .args(["check-ref-format", "--branch"])
        .arg(name)
        .accept_exit_code(128)
        .output(cancel)
        .await?;
    Ok(out.exit_code == Some(0) && out.stdout_lossy().trim_end() == name && name != "HEAD")
}

/// Creates a repository in `path` whose first branch is `branch`. Refuses
/// a folder that already is a repository (`git init` there would only
/// "reinitialize" it). Files already in the folder are left untracked.
pub async fn init_repository(
    git: &Git,
    path: &Path,
    branch: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let check = check_init(git, path, cancel).await?;
    if check.already_repository {
        return Err(GitError::AlreadyARepository(path.to_path_buf()));
    }
    if !valid_branch_name(git, path, branch, cancel).await? {
        return Err(GitError::InvalidBranchName(branch.to_owned()));
    }
    git.write(path)
        .args(["init", "--quiet"])
        .arg(format!("--initial-branch={branch}"))
        .output(cancel)
        .await?;
    Ok(())
}
