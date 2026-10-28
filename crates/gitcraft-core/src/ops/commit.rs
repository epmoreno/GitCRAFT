//! Reading a single commit: metadata, changed files and diffs.

use std::path::Path;

use serde::Serialize;

use crate::parse::commit::{CommitDetails, DETAILS_FORMAT, parse_commit_details};
use crate::parse::diff_tree::{FileChange, merge_counts, parse_name_status, parse_numstat};
use crate::parse::log::is_object_id;
use crate::{CancellationToken, Git, GitError, Result};

/// Commit ids come from our own graph, but they still cross the IPC
/// boundary: only hex object ids reach git's command line.
pub(crate) fn checked_oid(oid: &str) -> Result<&str> {
    if is_object_id(oid) {
        Ok(oid)
    } else {
        Err(GitError::Parse(format!("not a commit id: {oid:?}")))
    }
}

pub async fn commit_details(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<CommitDetails> {
    let oid = checked_oid(oid)?;
    let out = git
        .read(repo)
        .args([
            "show",
            "-s",
            "--no-show-signature",
            "--no-notes",
            DETAILS_FORMAT,
            oid,
            "--",
        ])
        .output(cancel)
        .await?;
    parse_commit_details(&out.stdout)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CommitChanges {
    /// What the commit is compared with: its first parent, or `None` for a
    /// root commit (compared with the empty tree). Merges are shown against
    /// their first parent.
    pub base: Option<String>,
    pub files: Vec<FileChange>,
}

/// The first parent of `oid`, or `None` for a root commit.
pub async fn first_parent(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let oid = checked_oid(oid)?;
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", &format!("{oid}^1")])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let parent = out.stdout_lossy().trim().to_owned();
    Ok((!parent.is_empty()).then_some(parent))
}

/// `diff-tree` arguments comparing `oid` with `base` (or the empty tree).
pub(crate) fn diff_tree_range(oid: &str, base: Option<&str>) -> Vec<String> {
    match base {
        Some(parent) => vec![parent.to_owned(), oid.to_owned()],
        None => vec!["--root".to_owned(), oid.to_owned()],
    }
}

/// Files changed by `oid`, with line counts and rename detection.
pub async fn commit_changes(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<CommitChanges> {
    let oid = checked_oid(oid)?;
    let base = first_parent(git, repo, oid, cancel).await?;
    let range = diff_tree_range(oid, base.as_deref());
    let list = |format: &'static str| {
        git.read(repo)
            .args(["diff-tree", "-r", "-z", "-M", "--no-commit-id", format])
            .args(&range)
            .arg("--")
            .output(cancel)
    };
    let (names, counts) = tokio::try_join!(list("--name-status"), list("--numstat"))?;
    let files = merge_counts(
        parse_name_status(&names.stdout)?,
        &parse_numstat(&counts.stdout)?,
    );
    Ok(CommitChanges { base, files })
}
