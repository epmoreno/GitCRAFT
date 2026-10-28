//! Diffs of uncommitted changes, and applying a selection of their hunks or
//! lines to the index.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ops::commit::finish_diff;
use crate::parse::diff::FileDiff;
use crate::patch::{Direction, HunkSelection, build_patch};
use crate::{CancellationToken, Git, GitCommand, GitError, Result};

/// Which uncommitted change of a file to diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum WorktreeArea {
    /// Working tree vs index.
    Unstaged,
    /// Index vs HEAD.
    Staged,
    /// A file git does not track yet (vs nothing).
    Untracked,
}

/// What to do with a selection of a file's changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SelectionAction {
    Stage,
    Unstage,
}

fn check_path(path: &str) -> Result<()> {
    if path.is_empty() || path.contains('\0') {
        return Err(GitError::Parse("invalid path".into()));
    }
    Ok(())
}

fn diff_command(
    git: &Git,
    repo: &Path,
    area: WorktreeArea,
    path: &str,
    old_path: Option<&str>,
) -> GitCommand {
    let common = ["--no-ext-diff", "--no-textconv", "--no-color", "-U3"];
    let cmd = git.read(repo).env("GIT_LITERAL_PATHSPECS", "1");
    match area {
        WorktreeArea::Unstaged => cmd.arg("diff").args(common).arg("--").arg(path),
        WorktreeArea::Staged => cmd
            .args(["diff", "--cached", "-M"])
            .args(common)
            .arg("--")
            .args(old_path)
            .arg(path),
        // Exit code 1 just means "there are differences".
        WorktreeArea::Untracked => cmd
            .args(["diff", "--no-index"])
            .args(common)
            .args(["--", "/dev/null", path])
            .accept_exit_code(1),
    }
}

/// The diff of one file's uncommitted change in `area`. Unless `full`,
/// large diffs come back as `too_large` without hunks.
pub async fn worktree_file_diff(
    git: &Git,
    repo: &Path,
    path: &str,
    old_path: Option<&str>,
    area: WorktreeArea,
    full: bool,
    cancel: &CancellationToken,
) -> Result<FileDiff> {
    check_path(path)?;
    let out = diff_command(git, repo, area, path, old_path)
        .output(cancel)
        .await?;
    finish_diff(&out.stdout, full)
}

/// Stages (from `Unstaged`/`Untracked`) or unstages (from `Staged`) the
/// selected hunks and lines of one file. Only the index changes.
///
/// `fingerprint` is the one of the diff the user saw: if the file changed
/// since, nothing is applied and [`GitError::StaleDiff`] is returned.
#[allow(clippy::too_many_arguments)]
pub async fn apply_selection(
    git: &Git,
    repo: &Path,
    path: &str,
    old_path: Option<&str>,
    area: WorktreeArea,
    fingerprint: &str,
    selection: &[HunkSelection],
    action: SelectionAction,
    cancel: &CancellationToken,
) -> Result<()> {
    let direction = match (action, area) {
        (SelectionAction::Stage, WorktreeArea::Unstaged | WorktreeArea::Untracked) => {
            Direction::Forward
        }
        (SelectionAction::Unstage, WorktreeArea::Staged) => Direction::Reverse,
        _ => return Err(GitError::Parse(format!("cannot {action:?} from {area:?}"))),
    };
    let diff = worktree_file_diff(git, repo, path, old_path, area, true, cancel).await?;
    if diff.fingerprint != fingerprint {
        return Err(GitError::StaleDiff);
    }
    if diff.binary {
        return Err(GitError::Parse("binary files are staged whole".into()));
    }
    let patch = build_patch(&diff, selection, direction)
        .ok_or_else(|| GitError::Parse("the selection contains no change".into()))?;
    let mut cmd = git
        .write(repo)
        .args(["apply", "--cached", "--whitespace=nowarn"]);
    if direction == Direction::Reverse {
        cmd = cmd.arg("-R");
    }
    cmd.arg("-").stdin(patch).output(cancel).await?;
    Ok(())
}
