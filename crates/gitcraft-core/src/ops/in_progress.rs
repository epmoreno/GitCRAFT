//! The merge, rebase, cherry-pick or revert waiting for the user: what each
//! side of its conflicts is, and how far a rebase has got.
//!
//! In a merge, cherry-pick or revert, "ours" is the current branch. In a
//! rebase it is the other way round: HEAD is the new base (plus the commits
//! already replayed) and "theirs" is your commit being replayed. These
//! types name the sides by what they are, so the UI never has to say
//! "ours" or "theirs".

use std::path::Path;

use serde::Serialize;

use crate::ops::status::in_progress;
use crate::parse::status::InProgress;
use crate::{CancellationToken, Git, GitError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CommitRef {
    pub oid: String,
    pub short: String,
    pub subject: String,
}

/// A commit and the branches (local first, then remote-tracking) that point
/// at it, if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct NamedCommit {
    pub commit: CommitRef,
    pub branches: Vec<String>,
}

/// Position in a rebase: replaying commit `current` of `total` (1-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RebaseProgress {
    pub current: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ConflictSides {
    /// Left: the current branch. Right: what is being merged into it.
    Merge {
        branch: Option<String>,
        head: CommitRef,
        merging: NamedCommit,
    },
    /// Left: the new base with the commits already replayed (HEAD is
    /// detached during a rebase). Right: your commit being replayed.
    Rebase {
        /// The branch being rebased (`None` if it was a detached HEAD).
        branch: Option<String>,
        onto: NamedCommit,
        head: CommitRef,
        /// `None` when git stopped without a commit being replayed (for
        /// example after an `exec` step).
        applying: Option<CommitRef>,
        progress: Option<RebaseProgress>,
    },
    /// Left: the current branch. Right: the commit being copied onto it.
    CherryPick {
        branch: Option<String>,
        head: CommitRef,
        picking: CommitRef,
    },
    /// Left: the current branch. Right: the changes that undo `reverting`.
    Revert {
        branch: Option<String>,
        head: CommitRef,
        reverting: CommitRef,
    },
}

/// `Some(oid)` if `rev` names a commit.
async fn commit_id(
    git: &Git,
    repo: &Path,
    rev: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "--end-of-options"])
        .arg(format!("{rev}^{{commit}}"))
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let oid = out.stdout_lossy().trim().to_owned();
    Ok((out.exit_code == Some(0) && !oid.is_empty()).then_some(oid))
}

async fn commit_ref(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<CommitRef> {
    let out = git
        .read(repo)
        .args([
            "show",
            "-s",
            "--no-show-signature",
            "--format=%H%x00%h%x00%s",
        ])
        .arg("--end-of-options")
        .arg(oid)
        .output(cancel)
        .await?;
    let text = out.stdout_lossy();
    let mut parts = text.trim_end_matches('\n').splitn(3, '\0');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(oid), Some(short), Some(subject)) if !oid.is_empty() => Ok(CommitRef {
            oid: oid.to_owned(),
            short: short.to_owned(),
            subject: subject.to_owned(),
        }),
        _ => Err(GitError::Parse(format!("commit summary: {text:?}"))),
    }
}

async fn named_commit(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<NamedCommit> {
    let commit = commit_ref(git, repo, oid, cancel).await?;
    let out = git
        .read(repo)
        .args(["for-each-ref", "--format=%(refname)%00"])
        .arg(format!("--points-at={oid}"))
        .args(["refs/heads", "refs/remotes"])
        .output(cancel)
        .await?;
    let text = out.stdout_lossy();
    let refs: Vec<&str> = text
        .split('\0')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .collect();
    let mut branches: Vec<String> = refs
        .iter()
        .filter_map(|r| r.strip_prefix("refs/heads/"))
        .map(str::to_owned)
        .collect();
    branches.extend(
        refs.iter()
            .filter_map(|r| r.strip_prefix("refs/remotes/"))
            .filter(|r| !r.ends_with("/HEAD"))
            .map(str::to_owned),
    );
    Ok(NamedCommit { commit, branches })
}

async fn current_branch(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(["symbolic-ref", "-q", "HEAD"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let name = out.stdout_lossy().trim().to_owned();
    Ok(name.strip_prefix("refs/heads/").map(str::to_owned))
}

/// The contents of a file git keeps in its directory while a rebase runs
/// (e.g. `rebase-merge/onto`), trimmed, if it exists.
async fn state_file(
    git: &Git,
    repo: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(["rev-parse", "--git-path", name])
        .output(cancel)
        .await?;
    let path = Path::new(out.stdout_lossy().trim()).to_path_buf();
    let path = if path.is_absolute() {
        path
    } else {
        repo.join(path)
    };
    Ok(std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned()))
}

async fn rebase_progress(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<Option<RebaseProgress>> {
    // `rebase-merge` (the default backend) or `rebase-apply` (`--apply`).
    for (current, total) in [
        ("rebase-merge/msgnum", "rebase-merge/end"),
        ("rebase-apply/next", "rebase-apply/last"),
    ] {
        let current = state_file(git, repo, current, cancel).await?;
        let total = state_file(git, repo, total, cancel).await?;
        if let (Some(current), Some(total)) = (current, total)
            && let (Ok(current), Ok(total)) = (current.parse(), total.parse())
        {
            return Ok(Some(RebaseProgress { current, total }));
        }
    }
    Ok(None)
}

async fn rebase_state(
    git: &Git,
    repo: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    for dir in ["rebase-merge", "rebase-apply"] {
        if let Some(value) = state_file(git, repo, &format!("{dir}/{name}"), cancel).await? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// The two sides of the operation waiting for the user, or `None` if there
/// is none.
pub async fn conflict_sides(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<Option<ConflictSides>> {
    let Some(operation) = in_progress(git, repo, cancel).await? else {
        return Ok(None);
    };
    let missing = |what: &str| GitError::Parse(format!("{what} not found during {operation:?}"));
    let head_oid = commit_id(git, repo, "HEAD", cancel)
        .await?
        .ok_or_else(|| missing("HEAD"))?;
    let head = commit_ref(git, repo, &head_oid, cancel).await?;

    let sides = match operation {
        InProgress::Merge => {
            let oid = commit_id(git, repo, "MERGE_HEAD", cancel)
                .await?
                .ok_or_else(|| missing("MERGE_HEAD"))?;
            ConflictSides::Merge {
                branch: current_branch(git, repo, cancel).await?,
                head,
                merging: named_commit(git, repo, &oid, cancel).await?,
            }
        }
        InProgress::Rebase => {
            let onto = rebase_state(git, repo, "onto", cancel)
                .await?
                .ok_or_else(|| missing("rebase onto"))?;
            let applying = match commit_id(git, repo, "REBASE_HEAD", cancel).await? {
                Some(oid) => Some(commit_ref(git, repo, &oid, cancel).await?),
                None => None,
            };
            let branch = rebase_state(git, repo, "head-name", cancel)
                .await?
                .and_then(|r| r.strip_prefix("refs/heads/").map(str::to_owned));
            ConflictSides::Rebase {
                branch,
                onto: named_commit(git, repo, &onto, cancel).await?,
                head,
                applying,
                progress: rebase_progress(git, repo, cancel).await?,
            }
        }
        InProgress::CherryPick => {
            let oid = commit_id(git, repo, "CHERRY_PICK_HEAD", cancel)
                .await?
                .ok_or_else(|| missing("CHERRY_PICK_HEAD"))?;
            ConflictSides::CherryPick {
                branch: current_branch(git, repo, cancel).await?,
                head,
                picking: commit_ref(git, repo, &oid, cancel).await?,
            }
        }
        InProgress::Revert => {
            let oid = commit_id(git, repo, "REVERT_HEAD", cancel)
                .await?
                .ok_or_else(|| missing("REVERT_HEAD"))?;
            ConflictSides::Revert {
                branch: current_branch(git, repo, cancel).await?,
                head,
                reverting: commit_ref(git, repo, &oid, cancel).await?,
            }
        }
    };
    Ok(Some(sides))
}
