//! Undoing and redoing an operation from its [`OpRecord`] (task 11.2).
//!
//! Both go the same way, in opposite directions: undo takes the refs from
//! where the operation left them back to where they were, and redo takes
//! them forward again. Either is offered only while HEAD and the local
//! branches and tags are exactly on the side it starts from.
//!
//! - **Uncommitted changes are always kept.** Commit, amend and soft or
//!   mixed resets move the branch with `reset --soft` (no file changes);
//!   the others with `reset --keep`, which only changes the files that
//!   differ between the two commits, with what was staged staged again.
//!   Checkouts go back with `switch`, renames with `branch --move`, and
//!   any other ref with `update-ref` and its expected value. Changes in the
//!   way go to the stash, only if the user accepts.
//! - **Exactly as before**: when undoing, if the files are also exactly as
//!   the operation left them, the index of before comes back too, with the
//!   copies the operation made (its stash entry and trashed files).
//! - **Never forced**: commits that reached a remote after the operation
//!   are not taken out of a branch (that would need a forced push).

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ops::branch::{BlockingFile, rename_branch};
use crate::ops::in_progress::CommitRef;
use crate::ops::journal::{HeadAt, OpKind, OpRecord, RefChange, current_fingerprint, current_refs};
use crate::ops::merge_rebase::{changed, checked_out_somewhere, commit_refs, in_the_way};
use crate::ops::reset::{ResetMode, put_back, remote_branches_containing};
use crate::ops::stash::create_stash;
use crate::ops::status::{in_progress, working_status};
use crate::{CancellationToken, Git, GitError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Direction {
    Undo,
    Redo,
}

/// Why it cannot be done; nothing was touched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum UndoRefusal {
    /// A merge, rebase, cherry-pick or revert is waiting.
    InProgress,
    /// HEAD, a branch or a tag is no longer where the operation (or, to
    /// redo, the undo) left it.
    Changed,
    /// These commits would leave `branch`, but reached a remote after the
    /// operation: taking them out would need a forced push.
    Published {
        branch: String,
        commits: Vec<CommitRef>,
        remotes: Vec<String>,
    },
    /// The branch to move is checked out in another worktree.
    CheckedOutElsewhere { branch: String },
}

/// A local branch that moves, is created or is deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BranchChange {
    /// Short name.
    pub branch: String,
    pub from: Option<String>,
    pub to: Option<String>,
    /// Commits that leave the branch, newest first (at most 50).
    pub leaving: Vec<CommitRef>,
    pub leaving_total: u32,
    /// Of those, the ones no other branch, tag or remote branch has: they
    /// will only stay in the reflog.
    pub only_in_reflog: u32,
    /// Commits that come into the branch, newest first (at most 50).
    pub arriving: Vec<CommitRef>,
    pub arriving_total: u32,
}

/// What undoing or redoing would do. Read-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct UndoPreview {
    pub direction: Direction,
    pub kind: OpKind,
    pub refused: Option<UndoRefusal>,
    pub branches: Vec<BranchChange>,
    pub tags: Vec<RefChange>,
    /// HEAD before and after, when it changes branch.
    pub head_from: HeadAt,
    pub head_to: HeadAt,
    /// Undo only: the files are exactly as the operation left them, so the
    /// index of before and the copies come back too.
    pub exact: bool,
    /// With `exact`: the stash entry that will be applied and the files that
    /// come back from the Trash.
    pub restores_stash: Option<String>,
    pub restores_trashed: Vec<String>,
    /// Without `exact`: the copy the operation made stays in the stash.
    pub kept_stash: Option<String>,
    /// Uncommitted changes in the way (they can go to the stash first).
    pub blocking: Vec<BlockingFile>,
    pub fingerprint: String,
}

/// What the user accepted in the dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct UndoRequest {
    pub direction: Direction,
    /// The preview's fingerprint: nothing is done if the repository changed.
    pub expected: String,
    /// Stash the changes in the way first.
    pub save_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOutcome {
    /// Done. `record` replaces the old one (its copies are spent once given
    /// back, and its fingerprints are the new ones).
    Done {
        record: Box<OpRecord>,
        /// The stash entry with the changes that were in the way.
        saved: Option<String>,
        exact: bool,
        /// What was staged could not be staged again (the changes are still
        /// in the files).
        unstaged: bool,
    },
    /// Nothing was done: the repository changed, it was refused, or changes
    /// are in the way and saving them was not accepted.
    NotDone { preview: Box<UndoPreview> },
}

/// Message of the stash made by "save my changes, then undo/redo".
pub fn undo_stash_message(direction: Direction) -> String {
    match direction {
        Direction::Undo => "GitCRAFT: antes de deshacer".to_owned(),
        Direction::Redo => "GitCRAFT: antes de rehacer".to_owned(),
    }
}

/// How the commit HEAD is on moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadMove {
    /// `reset --soft`: the files do not change.
    Soft,
    /// `reset --keep`: the files that differ between the commits change.
    Keep,
}

fn head_move(kind: &OpKind) -> HeadMove {
    match kind {
        OpKind::Commit { .. }
        | OpKind::Amend { .. }
        | OpKind::Reset {
            mode: ResetMode::Soft | ResetMode::Mixed,
            ..
        } => HeadMove::Soft,
        _ => HeadMove::Keep,
    }
}

/// The plan, from a record and a direction.
struct Plan {
    from_head: HeadAt,
    to_head: HeadAt,
    /// (refname, from, to) of each ref that changes.
    refs: Vec<(String, Option<String>, Option<String>)>,
}

fn plan(record: &OpRecord, direction: Direction) -> Plan {
    let refs = record
        .refs
        .iter()
        .map(|c| match direction {
            Direction::Undo => (c.refname.clone(), c.after.clone(), c.before.clone()),
            Direction::Redo => (c.refname.clone(), c.before.clone(), c.after.clone()),
        })
        .collect();
    let (from_head, to_head) = match direction {
        Direction::Undo => (record.head_after.clone(), record.head_before.clone()),
        Direction::Redo => (record.head_before.clone(), record.head_after.clone()),
    };
    Plan {
        from_head,
        to_head,
        refs,
    }
}

impl Plan {
    /// The commit HEAD moves from and to, when HEAD stays on the same branch
    /// (or stays detached) and that commit changes: `(from, to)`, either
    /// `None` for a branch without commits.
    fn head_commits(&self) -> Option<(Option<String>, Option<String>)> {
        match (&self.from_head, &self.to_head) {
            (HeadAt::Branch { name: a }, HeadAt::Branch { name: b }) if a == b => self
                .refs
                .iter()
                .find(|(refname, _, _)| refname == a)
                .map(|(_, from, to)| (from.clone(), to.clone())),
            (HeadAt::Detached { oid: a }, HeadAt::Detached { oid: b }) if a != b => {
                Some((Some(a.clone()), Some(b.clone())))
            }
            _ => None,
        }
    }

    /// HEAD goes to another branch, or between a branch and a commit.
    fn switches(&self) -> bool {
        match (&self.from_head, &self.to_head) {
            (HeadAt::Branch { name: a }, HeadAt::Branch { name: b }) => a != b,
            (HeadAt::Detached { .. }, HeadAt::Detached { .. }) => false,
            _ => true,
        }
    }

    fn current_branch(&self) -> Option<&str> {
        match &self.from_head {
            HeadAt::Branch { name } => Some(name),
            HeadAt::Detached { .. } => None,
        }
    }
}

fn short(refname: &str) -> &str {
    refname
        .strip_prefix("refs/heads/")
        .or_else(|| refname.strip_prefix("refs/tags/"))
        .unwrap_or(refname)
}

fn head_commit(
    head: &HeadAt,
    refs: &[(String, Option<String>, Option<String>)],
    to: bool,
) -> Option<String> {
    match head {
        HeadAt::Detached { oid } => Some(oid.clone()),
        HeadAt::Branch { name } => refs
            .iter()
            .find(|(refname, _, _)| refname == name)
            .map(|(_, from, target)| if to { target.clone() } else { from.clone() })
            .unwrap_or(None),
    }
}

async fn count(git: &Git, repo: &Path, args: &[&str], cancel: &CancellationToken) -> Result<u32> {
    let out = git
        .read(repo)
        .args(["rev-list", "--count"])
        .args(args)
        .output(cancel)
        .await?;
    Ok(out.stdout_lossy().trim().parse().unwrap_or(0))
}

async fn rev_list(
    git: &Git,
    repo: &Path,
    args: &[String],
    cancel: &CancellationToken,
) -> Result<BTreeSet<String>> {
    let out = git
        .read(repo)
        .args(["rev-list", "--ignore-missing"])
        .args(args)
        .output(cancel)
        .await?;
    Ok(out.stdout_lossy().lines().map(str::to_owned).collect())
}

async fn branch_change(
    git: &Git,
    repo: &Path,
    refname: &str,
    from: Option<&str>,
    to: Option<&str>,
    cancel: &CancellationToken,
) -> Result<BranchChange> {
    let name = short(refname);
    let mut change = BranchChange {
        branch: name.to_owned(),
        from: from.map(str::to_owned),
        to: to.map(str::to_owned),
        leaving: Vec::new(),
        leaving_total: 0,
        only_in_reflog: 0,
        arriving: Vec::new(),
        arriving_total: 0,
    };
    // With no other side (a branch created or deleted), against every
    // other branch, tag and remote branch.
    let exclude = format!("--exclude={name}");
    let others = [
        "--not",
        exclude.as_str(),
        "--branches",
        "--tags",
        "--remotes",
    ];
    let range = |tip: &str, other: Option<&str>| -> Vec<String> {
        let mut args = vec![tip.to_owned()];
        match other {
            Some(other) => args.push(format!("^{other}")),
            None => args.extend(others.iter().map(|a| (*a).to_owned())),
        }
        args
    };
    if let Some(from) = from {
        let range = range(from, to);
        let args: Vec<&str> = range.iter().map(String::as_str).collect();
        change.leaving = commit_refs(git, repo, &args, cancel).await?;
        change.leaving_total = count(git, repo, &args, cancel).await?;
        if change.leaving_total > 0 {
            let mut only = args.clone();
            if to.is_some() {
                only.extend(others);
            }
            change.only_in_reflog = count(git, repo, &only, cancel).await?;
        }
    }
    if let Some(to) = to {
        let range = range(to, from);
        let args: Vec<&str> = range.iter().map(String::as_str).collect();
        change.arriving = commit_refs(git, repo, &args, cancel).await?;
        change.arriving_total = count(git, repo, &args, cancel).await?;
    }
    Ok(change)
}

/// Commits that would leave `from` (for `to`) and reached a remote after
/// the operation: not reachable from the remote branches right after it.
async fn newly_published(
    git: &Git,
    repo: &Path,
    record: &OpRecord,
    from: &str,
    to: Option<&str>,
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    let mut leaving = vec![from.to_owned()];
    let mut remote = vec!["--remotes".to_owned()];
    let mut not = vec!["--not".to_owned()];
    not.extend(to.map(str::to_owned));
    not.extend(record.remotes_after.iter().cloned());
    leaving.extend(not.iter().cloned());
    remote.extend(not);
    let (leaving, remote) = tokio::try_join!(
        rev_list(git, repo, &leaving, cancel),
        rev_list(git, repo, &remote, cancel)
    )?;
    Ok(leaving.intersection(&remote).cloned().collect())
}

/// What undoing (or redoing) the operation would do now. Read-only.
pub async fn undo_preview(
    git: &Git,
    repo: &Path,
    record: &OpRecord,
    direction: Direction,
    cancel: &CancellationToken,
) -> Result<UndoPreview> {
    let plan = plan(record, direction);
    let fingerprint = current_fingerprint(git, repo, cancel).await?;
    let refs_now = current_refs(git, repo, cancel).await?;
    let starting = match direction {
        Direction::Undo => &record.refs_after,
        Direction::Redo => &record.refs_before,
    };
    let mut refused = if in_progress(git, repo, cancel).await?.is_some() {
        Some(UndoRefusal::InProgress)
    } else if &refs_now != starting {
        Some(UndoRefusal::Changed)
    } else {
        None
    };
    let exact = direction == Direction::Undo && fingerprint == record.fingerprint_after;

    let mut branches = Vec::new();
    let mut tags = Vec::new();
    for (refname, from, to) in &plan.refs {
        if refname.starts_with("refs/heads/") {
            if refused.is_none() {
                branches.push(
                    branch_change(git, repo, refname, from.as_deref(), to.as_deref(), cancel)
                        .await?,
                );
            }
        } else {
            tags.push(RefChange {
                refname: refname.clone(),
                before: from.clone(),
                after: to.clone(),
            });
        }
    }

    let renamed = matches!(record.kind, OpKind::RenameBranch { .. });
    if refused.is_none() {
        for (refname, from, to) in &plan.refs {
            if !refname.starts_with("refs/heads/") {
                continue;
            }
            if let Some(from) = from {
                let published =
                    newly_published(git, repo, record, from, to.as_deref(), cancel).await?;
                if let Some(first) = published.first() {
                    let args: Vec<&str> = published.iter().map(String::as_str).collect();
                    let mut args = args;
                    args.push("--no-walk");
                    refused = Some(UndoRefusal::Published {
                        branch: short(refname).to_owned(),
                        commits: commit_refs(git, repo, &args, cancel).await?,
                        remotes: remote_branches_containing(git, repo, first, cancel).await?,
                    });
                    break;
                }
            }
            // A branch other than the current one, open in another worktree.
            let current = plan.current_branch() == Some(refname.as_str())
                || matches!(&plan.to_head, HeadAt::Branch { name } if name == refname);
            if !current
                && !renamed
                && from.is_some()
                && checked_out_somewhere(git, repo, short(refname), cancel).await?
            {
                refused = Some(UndoRefusal::CheckedOutElsewhere {
                    branch: short(refname).to_owned(),
                });
                break;
            }
        }
    }

    // Uncommitted changes in the way of the files that change.
    let mut blocking = Vec::new();
    if refused.is_none() {
        let from = head_commit(&plan.from_head, &plan.refs, false);
        let to = head_commit(&plan.to_head, &plan.refs, true);
        let files_change = match plan.head_commits() {
            Some(_) => head_move(&record.kind) == HeadMove::Keep,
            None => plan.switches() && !renamed,
        };
        if files_change {
            // The commits HEAD is on now and will be on.
            let from = match &plan.from_head {
                HeadAt::Branch { name } if plan.head_commits().is_none() => {
                    current_tip(git, repo, name, cancel).await?
                }
                _ => from,
            };
            let to = match &plan.to_head {
                HeadAt::Branch { name } if plan.head_commits().is_none() => {
                    current_tip(git, repo, name, cancel).await?
                }
                _ => to,
            };
            if let (Some(from), Some(to)) = (from, to) {
                let touched = changed(git, repo, &from, &to, cancel).await?;
                let status = working_status(git, repo, cancel).await?;
                blocking = in_the_way(&status, &touched, false);
            }
        }
    }

    let (restores_stash, restores_trashed, kept_stash) = if exact {
        (
            record.stash.clone(),
            record.trashed.iter().map(|(p, _)| p.clone()).collect(),
            None,
        )
    } else if direction == Direction::Undo {
        (None, Vec::new(), record.stash.clone())
    } else {
        (None, Vec::new(), None)
    };

    Ok(UndoPreview {
        direction,
        kind: record.kind.clone(),
        refused,
        branches,
        tags,
        head_from: plan.from_head,
        head_to: plan.to_head,
        exact: exact && blocking.is_empty(),
        restores_stash,
        restores_trashed,
        kept_stash,
        blocking,
        fingerprint,
    })
}

async fn current_tip(
    git: &Git,
    repo: &Path,
    refname: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify"])
        .arg(format!("{refname}^{{commit}}"))
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let oid = out.stdout_lossy().trim().to_owned();
    Ok((!oid.is_empty()).then_some(oid))
}

/// What is staged, as a binary patch against HEAD (empty if nothing).
async fn staged_patch(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<Vec<u8>> {
    let out = git
        .read(repo)
        .args([
            "-c",
            "diff.noprefix=false",
            "-c",
            "diff.mnemonicPrefix=false",
            "diff",
            "--cached",
            "--binary",
            "--no-color",
            "--no-ext-diff",
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ])
        .output(cancel)
        .await?;
    Ok(out.stdout)
}

/// Stages `patch` again; `false` if it no longer applies (the changes are
/// still in the files).
async fn restage(
    git: &Git,
    repo: &Path,
    patch: Vec<u8>,
    cancel: &CancellationToken,
) -> Result<bool> {
    if patch.is_empty() {
        return Ok(true);
    }
    match git
        .write(repo)
        .args(["apply", "--cached", "--binary", "--whitespace=nowarn"])
        .stdin(patch)
        .output(cancel)
        .await
    {
        Ok(_) => Ok(true),
        Err(GitError::Cancelled) => Err(GitError::Cancelled),
        Err(_) => Ok(false),
    }
}

/// Moves the commit HEAD is on (its branch, or HEAD itself when detached).
async fn move_head(
    git: &Git,
    repo: &Path,
    plan: &Plan,
    how: HeadMove,
    kind: &OpKind,
    cancel: &CancellationToken,
) -> Result<bool> {
    let Some((from, to)) = plan.head_commits() else {
        return Ok(true);
    };
    let branch = match &plan.from_head {
        HeadAt::Branch { name } => Some(name.as_str()),
        HeadAt::Detached { .. } => None,
    };
    let (Some(_), Some(to)) = (from.clone(), to.clone()) else {
        // A branch without commits on one side (the first commit): only
        // the ref changes, the index and the files stay.
        let Some(branch) = branch else {
            return Err(GitError::Parse("detached HEAD without a commit".into()));
        };
        let line = match (from, to) {
            (Some(from), None) => format!("delete {branch} {from}\n"),
            (None, Some(to)) => format!("create {branch} {to}\n"),
            _ => return Ok(true),
        };
        update_refs(git, repo, &line, cancel).await?;
        return Ok(true);
    };
    let staged = staged_patch(git, repo, cancel).await?;
    match how {
        HeadMove::Soft => {
            git.write(repo)
                .args(["reset", "--quiet", "--soft", &to])
                .output(cancel)
                .await?;
            // A mixed reset also moved the index: it goes with the commit,
            // with what was staged on top.
            if matches!(
                kind,
                OpKind::Reset {
                    mode: ResetMode::Mixed,
                    ..
                }
            ) {
                git.write(repo)
                    .args(["read-tree", &to])
                    .output(cancel)
                    .await?;
                refresh(git, repo, cancel).await?;
                return restage(git, repo, staged, cancel).await;
            }
            Ok(true)
        }
        HeadMove::Keep => {
            git.write(repo)
                .args(["reset", "--quiet", "--keep", &to])
                .output(cancel)
                .await?;
            restage(git, repo, staged, cancel).await
        }
    }
}

async fn refresh(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<()> {
    git.write(repo)
        .args(["update-index", "-q", "--refresh"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    Ok(())
}

async fn update_refs(
    git: &Git,
    repo: &Path,
    lines: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    git.write(repo)
        .args(["update-ref", "-m", "GitCRAFT: undo", "--stdin"])
        .stdin(lines.as_bytes().to_vec())
        .output(cancel)
        .await?;
    Ok(())
}

/// Undoes or redoes the operation, if the repository is still as the
/// preview saw it. Never forces anything and never loses uncommitted
/// changes.
pub async fn undo(
    git: &Git,
    repo: &Path,
    record: &OpRecord,
    request: &UndoRequest,
    cancel: &CancellationToken,
) -> Result<UndoOutcome> {
    let preview = undo_preview(git, repo, record, request.direction, cancel).await?;
    let not_done = |preview: UndoPreview| UndoOutcome::NotDone {
        preview: Box::new(preview),
    };
    if preview.refused.is_some()
        || preview.fingerprint != request.expected
        || (!preview.blocking.is_empty() && !request.save_changes)
    {
        return Ok(not_done(preview));
    }
    let mut saved = None;
    if !preview.blocking.is_empty() {
        let message = undo_stash_message(request.direction);
        saved = Some(create_stash(git, repo, &message, true, cancel).await?);
    }
    let exact = preview.exact && saved.is_none();
    let plan = plan(record, request.direction);
    let renamed = matches!(record.kind, OpKind::RenameBranch { .. });

    // 1. HEAD: its commit, its branch, or a rename.
    let mut staging_kept = true;
    if renamed {
        if let [(old, _, _), (new, _, _)] = plan.refs.as_slice() {
            // Sorted by name: find which one exists now.
            let (now, then) = if plan.refs[0].1.is_some() {
                (old, new)
            } else {
                (new, old)
            };
            rename_branch(git, repo, short(now), short(then), cancel).await?;
        }
    } else if plan.switches() {
        match &plan.to_head {
            HeadAt::Branch { name } => {
                git.write(repo)
                    .args(["switch", "--quiet", "--no-guess", short(name)])
                    .output(cancel)
                    .await?;
            }
            HeadAt::Detached { oid } => {
                git.write(repo)
                    .args(["switch", "--quiet", "--detach", oid])
                    .output(cancel)
                    .await?;
            }
        }
    } else {
        staging_kept = move_head(
            git,
            repo,
            &plan,
            head_move(&record.kind),
            &record.kind,
            cancel,
        )
        .await?;
    }

    // 2. Every other ref, in one transaction with the expected values.
    let head_branch = plan.current_branch().map(str::to_owned);
    let mut lines = String::new();
    if !renamed {
        for (refname, from, to) in &plan.refs {
            if head_branch.as_deref() == Some(refname.as_str()) && plan.head_commits().is_some() {
                continue;
            }
            match (from, to) {
                (Some(from), Some(to)) => {
                    lines.push_str(&format!("update {refname} {to} {from}\n"))
                }
                (Some(from), None) => lines.push_str(&format!("delete {refname} {from}\n")),
                (None, Some(to)) => lines.push_str(&format!("create {refname} {to}\n")),
                (None, None) => {}
            }
        }
    }
    if !lines.is_empty() {
        update_refs(git, repo, &lines, cancel).await?;
    }

    // 3. Exactly as before: the index and the copies.
    let mut updated = record.clone();
    let moved_files = !renamed && (plan.head_commits().is_some() || plan.switches());
    if exact && moved_files {
        if let Some(stash) = &record.stash {
            git.write(repo)
                .args(["stash", "apply", "--index", "--quiet", stash])
                .output(cancel)
                .await?;
        } else if let Some(tree) = &record.index_before {
            git.write(repo)
                .args(["read-tree", tree])
                .output(cancel)
                .await?;
            refresh(git, repo, cancel).await?;
        }
        put_back(repo, &record.trashed);
        updated.stash = None;
        updated.trashed = Vec::new();
    }
    if request.direction == Direction::Redo {
        updated.fingerprint_after = current_fingerprint(git, repo, cancel).await?;
    }
    Ok(UndoOutcome::Done {
        record: Box::new(updated),
        saved,
        exact,
        unstaged: !staging_kept,
    })
}
