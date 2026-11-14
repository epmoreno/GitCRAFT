//! Resetting the current branch to a commit (task 10.4): the most delicate
//! operation of the app, so it is built like the discard.
//!
//! - **Preview first** (`reset_preview`): which commits leave the branch
//!   (and which of them no other branch, remote branch or tag has, and which
//!   are already published), what uncommitted work is lost, and which
//!   untracked files `reset --hard` would overwrite without warning (it
//!   does: files the target commit tracks).
//! - **A copy before**: what would be lost goes to a stash entry
//!   "GitCRAFT: antes de reset a <hash>" (index included, never dropped by
//!   itself), and untracked files that would be overwritten go to the Trash.
//!   The commits that leave the branch stay in git's reflog.
//! - **Nothing changes** if the repository is not as the preview saw it.
//! - **Undo** (`undo_reset`) puts everything back exactly, but only while
//!   nothing has changed since the reset.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ops::commit_actions::commit_actions;
use crate::ops::discard::{Trash, forget_trashed};
use crate::ops::in_progress::{CommitRef, commit_ref};
use crate::ops::status::working_status;
use crate::parse::diff::fingerprint;
use crate::{CancellationToken, Git, GitError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ResetMode {
    /// Only the branch moves: the commits' changes stay staged.
    Soft,
    /// The branch and the index move: the changes stay, unstaged.
    Mixed,
    /// Branch, index and working tree move: uncommitted work is discarded.
    Hard,
}

impl ResetMode {
    fn flag(self) -> &'static str {
        match self {
            Self::Soft => "--soft",
            Self::Mixed => "--mixed",
            Self::Hard => "--hard",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ResetRefusal {
    InProgress,
    Detached,
    /// The branch is already at that commit.
    IsHead,
}

/// A commit that stops being in the branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LeavingCommit {
    pub commit: CommitRef,
    /// No other branch, remote branch or tag has it: after the reset only
    /// the reflog (and "Undo") keeps it.
    pub only_here: bool,
    /// A remote branch has it: undoing it there needs a forced push, which
    /// affects whoever already has it.
    pub published: bool,
}

/// Commits listed at most; `leaving_total` says how many there are.
const MAX_LISTED: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ResetPreview {
    pub mode: ResetMode,
    pub branch: Option<String>,
    pub target: CommitRef,
    pub refused: Option<ResetRefusal>,
    /// Newest first, at most [`MAX_LISTED`].
    pub leaving: Vec<LeavingCommit>,
    pub leaving_total: u32,
    /// Remote branches that have some of the leaving commits.
    pub published_in: Vec<String>,
    /// Hard: tracked files whose uncommitted changes are discarded.
    pub lost_changes: Vec<String>,
    /// Mixed: staged files that end up unstaged (their content stays).
    pub unstaged_by_reset: Vec<String>,
    /// Hard: untracked (or ignored) files the target tracks, which git would
    /// overwrite: they go to the Trash first.
    pub overwritten_untracked: Vec<String>,
    /// Whether a stash entry will hold a copy of the uncommitted work.
    pub saves_stash: bool,
    /// Identifies the state the preview saw (HEAD and the uncommitted
    /// files): the reset only happens if it is still the same.
    pub fingerprint: String,
}

/// Everything needed to undo a reset; kept by the app, not shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ResetRecord {
    pub mode: ResetMode,
    pub branch: String,
    /// The commit the branch was on.
    pub previous: String,
    pub target: String,
    /// The stash entry with the uncommitted work, if one was made.
    pub stash: Option<String>,
    /// Untracked files moved to the Trash: (path in the repo, where it is).
    pub trashed: Vec<(String, PathBuf)>,
    /// The state right after the reset: undo only if it is still this.
    pub fingerprint_after: String,
}

/// Message of the stash entry made before a reset.
pub fn reset_stash_message(short: &str) -> String {
    format!("GitCRAFT: antes de reset a {short}")
}

fn split_z(stdout: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

async fn rev_list(
    git: &Git,
    repo: &Path,
    args: &[&str],
    cancel: &CancellationToken,
) -> Result<BTreeSet<String>> {
    let out = git
        .read(repo)
        .arg("rev-list")
        .args(args)
        .output(cancel)
        .await?;
    Ok(out.stdout_lossy().lines().map(str::to_owned).collect())
}

/// HEAD and every uncommitted path with its size and modification time: a
/// cheap fingerprint of "the repository as it is now".
pub(crate) async fn state_fingerprint(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<String> {
    let head = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "HEAD"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let status = git
        .read(repo)
        .args(["status", "--porcelain=v2", "-z", "--untracked-files=all"])
        .output(cancel)
        .await?;
    let mut text = head.stdout_lossy();
    text.push_str(&String::from_utf8_lossy(&status.stdout));
    for record in split_z(&status.stdout) {
        // The path is the last space-separated field (renames add another
        // NUL-separated one, which is also a record here).
        let path = record.rsplit(' ').next().unwrap_or(&record);
        if let Ok(meta) = std::fs::symlink_metadata(repo.join(path)) {
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            text.push_str(&format!("\0{path}\0{}\0{modified}", meta.len()));
        }
    }
    Ok(fingerprint(text.as_bytes()))
}

/// The remote branches (short names, like `origin/main`) that have `oid`.
pub(crate) async fn remote_branches_containing(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    let out = git
        .read(repo)
        .args(["for-each-ref", "--format=%(refname)"])
        .arg(format!("--contains={oid}"))
        .arg("refs/remotes")
        .output(cancel)
        .await?;
    // Full names, so `origin/HEAD` (short: "origin") is left out.
    Ok(out
        .stdout_lossy()
        .lines()
        .filter(|r| !r.ends_with("/HEAD"))
        .filter_map(|r| r.strip_prefix("refs/remotes/"))
        .map(str::to_owned)
        .collect())
}

/// What resetting the current branch to `oid` with `mode` would do.
/// Read-only.
pub async fn reset_preview(
    git: &Git,
    repo: &Path,
    oid: &str,
    mode: ResetMode,
    cancel: &CancellationToken,
) -> Result<ResetPreview> {
    let facts = commit_actions(git, repo, oid, cancel).await?;
    let target = commit_ref(git, repo, &facts.oid, cancel).await?;
    let refused = if facts.in_progress.is_some() {
        Some(ResetRefusal::InProgress)
    } else if facts.branch.is_none() {
        Some(ResetRefusal::Detached)
    } else if facts.is_head {
        Some(ResetRefusal::IsHead)
    } else {
        None
    };
    let mut preview = ResetPreview {
        mode,
        branch: facts.branch.clone(),
        target,
        refused,
        leaving: Vec::new(),
        leaving_total: 0,
        published_in: Vec::new(),
        lost_changes: Vec::new(),
        unstaged_by_reset: Vec::new(),
        overwritten_untracked: Vec::new(),
        saves_stash: false,
        fingerprint: state_fingerprint(git, repo, cancel).await?,
    };
    let Some(branch) = facts.branch.filter(|_| refused.is_none()) else {
        return Ok(preview);
    };

    // Commits that leave the branch, and where else they are.
    let range = format!("{}..HEAD", facts.oid);
    // Newest first, as git lists them.
    let leaving: Vec<String> = git
        .read(repo)
        .args(["rev-list", &range])
        .output(cancel)
        .await?
        .stdout_lossy()
        .lines()
        .map(str::to_owned)
        .collect();
    // With --branches, --exclude takes the name without "refs/heads/".
    let exclude = format!("--exclude={branch}");
    let only_here = rev_list(
        git,
        repo,
        &[
            &range,
            "--not",
            &exclude,
            "--branches",
            "--remotes",
            "--tags",
        ],
        cancel,
    )
    .await?;
    let not_published = rev_list(git, repo, &[&range, "--not", "--remotes"], cancel).await?;
    preview.leaving_total = u32::try_from(leaving.len()).unwrap_or(u32::MAX);
    let log = git
        .read(repo)
        .args([
            "log",
            "-z",
            "--no-show-signature",
            "--format=%H%x1f%h%x1f%s",
        ])
        .arg(format!("--max-count={MAX_LISTED}"))
        .arg(&range)
        .output(cancel)
        .await?;
    for record in split_z(&log.stdout) {
        let mut parts = record.trim_start_matches('\n').splitn(3, '\u{1f}');
        let (Some(oid), Some(short), Some(subject)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(GitError::Parse(format!("log record: {record:?}")));
        };
        preview.leaving.push(LeavingCommit {
            commit: CommitRef {
                oid: oid.to_owned(),
                short: short.to_owned(),
                subject: subject.to_owned(),
            },
            only_here: only_here.contains(oid),
            published: !not_published.contains(oid),
        });
    }
    if let Some(oldest_published) = leaving
        .iter()
        .rev()
        .find(|oid| !not_published.contains(*oid))
    {
        preview.published_in =
            remote_branches_containing(git, repo, oldest_published, cancel).await?;
    }

    // Uncommitted work.
    let status = working_status(git, repo, cancel).await?;
    match mode {
        ResetMode::Soft => {}
        ResetMode::Mixed => {
            preview.unstaged_by_reset = status.staged.iter().map(|e| e.path.clone()).collect();
            preview.saves_stash = !preview.unstaged_by_reset.is_empty();
        }
        ResetMode::Hard => {
            let mut lost: BTreeSet<String> = BTreeSet::new();
            for e in status.staged.iter().chain(&status.unstaged) {
                lost.insert(e.path.clone());
                lost.extend(e.old_path.clone());
            }
            preview.lost_changes = lost.into_iter().collect();
            preview.saves_stash = !preview.lost_changes.is_empty();
            // Untracked and ignored files the target tracks.
            let tracked_there = git
                .read(repo)
                .args(["ls-tree", "-r", "-z", "--name-only", &facts.oid])
                .output(cancel)
                .await?;
            let tracked_there: BTreeSet<String> =
                split_z(&tracked_there.stdout).into_iter().collect();
            let others = git
                .read(repo)
                .args(["ls-files", "-o", "-z"])
                .output(cancel)
                .await?;
            preview.overwritten_untracked = split_z(&others.stdout)
                .into_iter()
                .filter(|p| tracked_there.contains(p))
                .collect();
        }
    }
    Ok(preview)
}

/// Resets the current branch to `oid` with `mode`, if the repository is
/// still as the preview (`expected`) saw it. Saves a copy of what would be
/// lost first (see the module docs). Returns how to undo it.
pub async fn reset(
    git: &Git,
    repo: &Path,
    oid: &str,
    mode: ResetMode,
    expected: &str,
    trash: &dyn Trash,
    cancel: &CancellationToken,
) -> Result<ResetRecord> {
    let preview = reset_preview(git, repo, oid, mode, cancel).await?;
    if preview.refused.is_some() || preview.fingerprint != expected {
        return Err(GitError::StaleDiff);
    }
    let branch = preview.branch.clone().unwrap_or_default();
    let previous = git
        .read(repo)
        .args(["rev-parse", "HEAD"])
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned();

    // 1. A copy of the uncommitted work, index included.
    let mut stash = None;
    if preview.saves_stash {
        let message = reset_stash_message(&preview.target.short);
        let created = git
            .write(repo)
            .args(["stash", "create", &message])
            .output(cancel)
            .await?;
        let oid = created.stdout_lossy().trim().to_owned();
        if !oid.is_empty() {
            git.write(repo)
                .args(["stash", "store", "-m", &message, &oid])
                .output(cancel)
                .await?;
            stash = Some(oid);
        }
    }

    // 2. Untracked files git would overwrite, to the Trash.
    let mut trashed: Vec<(String, PathBuf)> = Vec::new();
    for path in &preview.overwritten_untracked {
        match trash.trash(&repo.join(path)) {
            Ok(to) => trashed.push((path.clone(), to)),
            Err(e) => {
                put_back(repo, &trashed);
                return Err(GitError::TrashFailed {
                    path: path.clone(),
                    reason: e.to_string(),
                });
            }
        }
    }

    // 3. The reset itself.
    let done = git
        .write(repo)
        .args(["reset", "--quiet", mode.flag(), &preview.target.oid])
        .output(cancel)
        .await;
    if let Err(e) = done {
        put_back(repo, &trashed);
        return Err(e);
    }
    Ok(ResetRecord {
        mode,
        branch,
        previous,
        target: preview.target.oid,
        stash,
        trashed,
        fingerprint_after: state_fingerprint(git, repo, cancel).await?,
    })
}

/// Best effort: moves trashed files back to where they were.
pub(crate) fn put_back(repo: &Path, trashed: &[(String, PathBuf)]) {
    for (path, to) in trashed {
        let original = repo.join(path);
        if let Some(parent) = original.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::rename(to, &original).is_ok() {
            let _ = forget_trashed(to);
        }
    }
}

/// Puts the branch, the index, the working tree and the trashed files back
/// as they were before the reset, exactly. Refuses, changing nothing, if
/// anything changed since the reset (`GitError::UndoConflict`).
pub async fn undo_reset(
    git: &Git,
    repo: &Path,
    record: &ResetRecord,
    cancel: &CancellationToken,
) -> Result<()> {
    let branch = git
        .read(repo)
        .args(["symbolic-ref", "-q", "--short", "HEAD"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    if branch.stdout_lossy().trim() != record.branch
        || state_fingerprint(git, repo, cancel).await? != record.fingerprint_after
    {
        return Err(GitError::UndoConflict(record.branch.clone()));
    }
    let reset = |flag: &'static str| {
        git.write(repo)
            .args(["reset", "--quiet", flag, &record.previous])
            .output(cancel)
    };
    match record.mode {
        ResetMode::Soft => {
            reset("--soft").await?;
        }
        ResetMode::Mixed => {
            reset("--mixed").await?;
            if let Some(stash) = &record.stash {
                // The index as it was: the stash's index commit.
                git.write(repo)
                    .args(["read-tree", &format!("{stash}^2")])
                    .output(cancel)
                    .await?;
            }
        }
        ResetMode::Hard => {
            reset("--hard").await?;
            if let Some(stash) = &record.stash {
                git.write(repo)
                    .args(["stash", "apply", "--index", "--quiet", stash])
                    .output(cancel)
                    .await?;
            }
            put_back(repo, &record.trashed);
        }
    }
    Ok(())
}
