//! Cherry-pick (task 10.2) and revert (task 10.3): applying one commit's
//! changes onto the current branch, forwards (a copy) or backwards (a new
//! commit that undoes them). Like the pull, they never force: before
//! touching anything they say why they cannot be done, which uncommitted
//! changes are in the way and which files would conflict. One that stops on
//! conflicts is resolved with the conflict view (task 9).

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::ops::branch::BlockingFile;
use crate::ops::commit_actions::commit_actions;
use crate::ops::in_progress::{CommitRef, commit_ref};
use crate::ops::stash::create_stash;
use crate::ops::status::working_status;
use crate::{CancellationToken, Git, Result};

/// Which way the commit's changes are applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PickKind {
    /// A copy of the commit (`git cherry-pick`).
    CherryPick,
    /// A new commit that undoes it (`git revert`).
    Revert,
}

/// Why the commit cannot be picked (or reverted) at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PickRefusal {
    /// A merge, rebase… is waiting for the user.
    InProgress,
    /// HEAD is not on a branch.
    Detached,
    /// A merge commit: one parent would have to be chosen as the main line.
    Merge,
    /// Cherry-pick: the commit is already in the current branch.
    AlreadyInBranch,
    /// Revert: the commit is not in the current branch.
    NotInBranch,
}

/// Why uncommitted changes are in the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PickBlock {
    /// git refuses with anything staged, even in other files.
    StagedChanges,
    /// Changes (or untracked files) in files the commit changes.
    Overwrite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PickPreview {
    pub kind: PickKind,
    pub commit: CommitRef,
    /// The current branch (`None` with a detached HEAD).
    pub branch: Option<String>,
    pub refused: Option<PickRefusal>,
    /// Uncommitted changes in the way.
    pub blocked: Option<PickBlock>,
    pub blocking: Vec<BlockingFile>,
    /// Files that would stop with conflicts.
    pub conflicts: Vec<String>,
    /// `conflicts` comes from git's own merge (git ≥ 2.40); otherwise it is
    /// an estimate (files changed on both sides).
    pub exact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
///
/// In every outcome, `saved` is the stash entry (commit id) holding the
/// changes "save my changes" put aside, so the app can offer them back.
pub enum PickOutcome {
    /// The commit was copied: `head` is the new commit.
    Done { head: String, saved: Option<String> },
    /// Started at the user's request and stopped on conflicts in these
    /// files: a cherry-pick is waiting (task 9's banner and view).
    Conflicted {
        files: Vec<String>,
        saved: Option<String>,
    },
    /// Nothing was picked; the preview says why (refused, blocked, or
    /// conflicts the user did not accept).
    NotDone {
        preview: PickPreview,
        saved: Option<String>,
    },
}

fn names(stdout: &[u8]) -> BTreeSet<String> {
    String::from_utf8_lossy(stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The tree the commit's changes start from: its parent, or the empty tree
/// for a root commit.
async fn base_of(git: &Git, repo: &Path, oid: &str, cancel: &CancellationToken) -> Result<String> {
    let parent = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "--end-of-options"])
        .arg(format!("{oid}^1"))
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let parent = parent.stdout_lossy().trim().to_owned();
    if !parent.is_empty() {
        return Ok(parent);
    }
    let empty = git
        .read(repo)
        .args(["hash-object", "-t", "tree", "--stdin"])
        .stdin(Vec::new())
        .output(cancel)
        .await?;
    Ok(empty.stdout_lossy().trim().to_owned())
}

async fn changed(
    git: &Git,
    repo: &Path,
    from: &str,
    to: &str,
    cancel: &CancellationToken,
) -> Result<BTreeSet<String>> {
    let out = git
        .read(repo)
        .args(["diff", "--name-only", "-z", "--no-renames", from, to, "--"])
        .output(cancel)
        .await?;
    Ok(names(&out.stdout))
}

/// Files that would conflict applying `base`→`theirs` onto HEAD.
async fn predicted_conflicts(
    git: &Git,
    repo: &Path,
    base: &str,
    theirs: &str,
    cancel: &CancellationToken,
) -> Result<(Vec<String>, bool)> {
    let merge = git
        .read(repo)
        .args([
            "merge-tree",
            "--write-tree",
            "--name-only",
            "--no-messages",
            "-z",
        ])
        .arg(format!("--merge-base={base}"))
        .args(["HEAD", theirs])
        .accept_exit_code(1)
        .output(cancel)
        .await;
    if let Ok(out) = merge {
        if out.exit_code == Some(0) {
            return Ok((Vec::new(), true));
        }
        let files: BTreeSet<String> = String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .skip(1)
            .take_while(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        return Ok((files.into_iter().collect(), true));
    }
    // Older git: the files both the commit and HEAD changed since `base`.
    let ours = changed(git, repo, base, "HEAD", cancel).await?;
    let theirs = changed(git, repo, base, theirs, cancel).await?;
    Ok((ours.intersection(&theirs).cloned().collect(), false))
}

/// What cherry-picking `oid` onto the current branch would do. Read-only.
pub async fn cherry_pick_preview(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<PickPreview> {
    pick_preview(git, repo, PickKind::CherryPick, oid, cancel).await
}

/// What reverting `oid` (a commit of the current branch) would do.
/// Read-only.
pub async fn revert_preview(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<PickPreview> {
    pick_preview(git, repo, PickKind::Revert, oid, cancel).await
}

async fn pick_preview(
    git: &Git,
    repo: &Path,
    kind: PickKind,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<PickPreview> {
    let facts = commit_actions(git, repo, oid, cancel).await?;
    let commit = commit_ref(git, repo, &facts.oid, cancel).await?;
    let refused = if facts.in_progress.is_some() {
        Some(PickRefusal::InProgress)
    } else if facts.branch.is_none() {
        Some(PickRefusal::Detached)
    } else if facts.is_merge {
        Some(PickRefusal::Merge)
    } else if kind == PickKind::CherryPick && facts.in_branch {
        Some(PickRefusal::AlreadyInBranch)
    } else if kind == PickKind::Revert && !facts.in_branch {
        Some(PickRefusal::NotInBranch)
    } else {
        None
    };
    let mut preview = PickPreview {
        kind,
        commit,
        branch: facts.branch,
        refused,
        blocked: None,
        blocking: Vec::new(),
        conflicts: Vec::new(),
        exact: true,
    };
    if refused.is_some() {
        return Ok(preview);
    }

    let base = base_of(git, repo, &facts.oid, cancel).await?;
    let touched = changed(git, repo, &base, &facts.oid, cancel).await?;
    let status = working_status(git, repo, cancel).await?;
    if !status.staged.is_empty() {
        preview.blocked = Some(PickBlock::StagedChanges);
        preview.blocking = status
            .staged
            .iter()
            .map(|e| BlockingFile {
                path: e.path.clone(),
                untracked: false,
            })
            .collect();
    } else {
        let mut blocking: Vec<BlockingFile> = status
            .unstaged
            .iter()
            .filter(|e| touched.contains(&e.path))
            .map(|e| BlockingFile {
                path: e.path.clone(),
                untracked: false,
            })
            .collect();
        blocking.extend(
            status
                .untracked
                .iter()
                .filter(|e| {
                    if e.path.ends_with('/') {
                        touched.iter().any(|p| p.starts_with(e.path.as_str()))
                    } else {
                        touched.contains(&e.path)
                    }
                })
                .map(|e| BlockingFile {
                    path: e.path.clone(),
                    untracked: true,
                }),
        );
        if !blocking.is_empty() {
            preview.blocked = Some(PickBlock::Overwrite);
            preview.blocking = blocking;
        }
    }
    // A revert applies the commit's changes backwards: from the commit to
    // its parent.
    let (from, to) = match kind {
        PickKind::CherryPick => (base.as_str(), facts.oid.as_str()),
        PickKind::Revert => (facts.oid.as_str(), base.as_str()),
    };
    let (conflicts, exact) = predicted_conflicts(git, repo, from, to, cancel).await?;
    preview.conflicts = conflicts;
    preview.exact = exact;
    Ok(preview)
}

/// Message of the stash made by "save my changes, then cherry-pick".
pub fn pick_stash_message(short: &str) -> String {
    format!("GitCRAFT: antes del cherry-pick de {short}")
}

/// Message of the stash made by "save my changes, then revert".
pub fn revert_stash_message(short: &str) -> String {
    format!("GitCRAFT: antes de revertir {short}")
}

/// Cherry-picks `oid` onto the current branch. It checks everything again
/// first and does nothing if the commit is refused, if changes are in the
/// way (unless `save_changes`: they are stashed first, in an entry that is
/// never dropped automatically) or if it would conflict (unless
/// `allow_conflicts`).
pub async fn cherry_pick(
    git: &Git,
    repo: &Path,
    oid: &str,
    save_changes: bool,
    allow_conflicts: bool,
    cancel: &CancellationToken,
) -> Result<PickOutcome> {
    apply_commit(
        git,
        repo,
        PickKind::CherryPick,
        oid,
        save_changes,
        allow_conflicts,
        cancel,
    )
    .await
}

/// Reverts `oid` with a new commit (`git revert --no-edit`, git's
/// «Revert "…"» message), with the same checks as [`cherry_pick`].
pub async fn revert(
    git: &Git,
    repo: &Path,
    oid: &str,
    save_changes: bool,
    allow_conflicts: bool,
    cancel: &CancellationToken,
) -> Result<PickOutcome> {
    apply_commit(
        git,
        repo,
        PickKind::Revert,
        oid,
        save_changes,
        allow_conflicts,
        cancel,
    )
    .await
}

async fn apply_commit(
    git: &Git,
    repo: &Path,
    kind: PickKind,
    oid: &str,
    save_changes: bool,
    allow_conflicts: bool,
    cancel: &CancellationToken,
) -> Result<PickOutcome> {
    let mut preview = pick_preview(git, repo, kind, oid, cancel).await?;
    let mut saved = None;
    if preview.refused.is_some() {
        return Ok(PickOutcome::NotDone { preview, saved });
    }
    if preview.blocked.is_some() {
        if !save_changes {
            return Ok(PickOutcome::NotDone { preview, saved });
        }
        let message = match kind {
            PickKind::CherryPick => pick_stash_message(&preview.commit.short),
            PickKind::Revert => revert_stash_message(&preview.commit.short),
        };
        saved = Some(create_stash(git, repo, &message, true, cancel).await?);
        preview = pick_preview(git, repo, kind, oid, cancel).await?;
        if preview.refused.is_some() || preview.blocked.is_some() {
            return Ok(PickOutcome::NotDone { preview, saved });
        }
    }
    if !preview.conflicts.is_empty() && !allow_conflicts {
        return Ok(PickOutcome::NotDone { preview, saved });
    }
    let command: &[&str] = match kind {
        PickKind::CherryPick => &["cherry-pick"],
        PickKind::Revert => &["revert", "--no-edit"],
    };
    let out = git
        .write(repo)
        .args(command)
        .arg("--end-of-options")
        .arg(&preview.commit.oid)
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let status = working_status(git, repo, cancel).await?;
    if out.exit_code == Some(1) || !status.conflicted.is_empty() {
        return Ok(PickOutcome::Conflicted {
            files: status.conflicted.into_iter().map(|c| c.path).collect(),
            saved,
        });
    }
    let head = git
        .read(repo)
        .args(["rev-parse", "HEAD"])
        .output(cancel)
        .await?;
    Ok(PickOutcome::Done {
        head: head.stdout_lossy().trim().to_owned(),
        saved,
    })
}
