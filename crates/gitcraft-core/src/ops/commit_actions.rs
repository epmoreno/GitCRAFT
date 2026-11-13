//! What can be done with a commit of the graph (task 10): the facts the UI
//! needs to offer cherry-pick, revert, reset and tag, or to say why not.
//! Read-only.

use std::path::Path;

use serde::Serialize;

use crate::ops::status::in_progress;
use crate::parse::status::InProgress;
use crate::{CancellationToken, Git, GitError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CommitActions {
    pub oid: String,
    /// The branch HEAD is on (`None`: detached HEAD, or no commit yet).
    pub branch: Option<String>,
    /// A merge, rebase… waiting for the user: nothing else can start.
    pub in_progress: Option<InProgress>,
    /// More than one parent: cherry-pick and revert are not offered.
    pub is_merge: bool,
    /// The commit is HEAD or one of its ancestors (already in the branch).
    pub in_branch: bool,
    /// The commit is HEAD itself.
    pub is_head: bool,
}

fn checked_oid(oid: &str) -> Result<&str> {
    let ok = (4..=64).contains(&oid.len()) && oid.bytes().all(|b| b.is_ascii_hexdigit());
    if ok {
        Ok(oid)
    } else {
        Err(GitError::Parse(format!("not a commit id: {oid:?}")))
    }
}

/// The facts about `oid` that decide which actions are offered.
pub async fn commit_actions(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<CommitActions> {
    let oid = checked_oid(oid)?;
    let parents = git
        .read(repo)
        .args(["rev-list", "--no-walk", "--parents", "--end-of-options"])
        .arg(oid)
        .output(cancel)
        .await?;
    let ids: Vec<String> = parents
        .stdout_lossy()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let full = ids
        .first()
        .cloned()
        .ok_or_else(|| GitError::Parse(format!("commit {oid} not found")))?;

    let branch = git
        .read(repo)
        .args(["symbolic-ref", "-q", "--short", "HEAD"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let branch = branch.stdout_lossy().trim().to_owned();
    let head = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "HEAD^{commit}"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let head = head.stdout_lossy().trim().to_owned();
    let has_head = !head.is_empty();

    let in_branch = has_head && {
        let out = git
            .read(repo)
            .args(["merge-base", "--is-ancestor", &full, "HEAD"])
            .accept_exit_code(1)
            .output(cancel)
            .await?;
        out.exit_code == Some(0)
    };

    Ok(CommitActions {
        branch: (has_head && !branch.is_empty()).then_some(branch),
        in_progress: in_progress(git, repo, cancel).await?,
        is_merge: ids.len() > 2,
        is_head: head == full,
        in_branch,
        oid: full,
    })
}
