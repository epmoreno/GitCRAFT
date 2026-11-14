//! The journal of operations (task 11.1): what an operation changed, so it
//! can be undone and redone.
//!
//! It works the same for every operation: a [`Snapshot`] right before it
//! (HEAD, every local branch and tag, and the index as a tree), and
//! [`record`] right after, which compares and returns an [`OpRecord`] with
//! the refs that moved. Operations that move no local ref and leave HEAD
//! where it was (staging, stash, discards, fetch, push) return nothing: they
//! are not undone from here.
//!
//! Old commits are not copied anywhere: they stay in git's reflog, which
//! also keeps them from being garbage collected.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ops::reset::{ResetMode, state_fingerprint};
use crate::parse::diff::fingerprint;
use crate::{CancellationToken, Git, GitError, Result};

/// What the operation was, for its texts ("Undo commit «x»").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum OpKind {
    Commit {
        summary: String,
    },
    Amend {
        summary: String,
    },
    /// A merge into the current branch, or the fast-forward of a branch.
    Merge {
        source: String,
        fast_forward: bool,
    },
    Rebase {
        onto: String,
    },
    Reset {
        mode: ResetMode,
        target: String,
    },
    CherryPick {
        summary: String,
    },
    Revert {
        summary: String,
    },
    /// The local part of a pull (its merge or rebase).
    Pull {
        branch: String,
    },
    Checkout {
        branch: String,
    },
    CreateBranch {
        name: String,
    },
    RenameBranch {
        old: String,
        new: String,
    },
    DeleteBranch {
        name: String,
    },
    CreateTag {
        name: String,
    },
    DeleteTag {
        name: String,
    },
}

/// Where HEAD points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum HeadAt {
    /// A branch (full ref name), also when it has no commits yet.
    Branch {
        name: String,
    },
    Detached {
        oid: String,
    },
}

/// A local ref the operation created, moved or deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RefChange {
    /// Full ref name (`refs/heads/…`, `refs/tags/…`).
    pub refname: String,
    /// `None`: it did not exist (before) or no longer exists (after).
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Everything needed to undo an operation; kept by the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OpRecord {
    pub kind: OpKind,
    /// Sorted by ref name.
    pub refs: Vec<RefChange>,
    pub head_before: HeadAt,
    pub head_after: HeadAt,
    /// The index as a tree right before (with what was staged); `None` if
    /// it could not be written (conflicts).
    pub index_before: Option<String>,
    /// The stash entry the operation made with the user's changes, if any.
    pub stash: Option<String>,
    /// Untracked files it moved to the Trash: (path in the repo, where it is).
    pub trashed: Vec<(String, PathBuf)>,
    /// HEAD and the local branches and tags right before: an undone
    /// operation can be redone while they are still these.
    pub refs_before: String,
    /// HEAD and the local branches and tags right after: the operation can
    /// be undone while they are still these.
    pub refs_after: String,
    /// The remote-tracking branches right after (their commits): commits
    /// that reach a remote only later were published after the operation,
    /// and undoing it would need a forced push.
    pub remotes_after: Vec<String>,
    /// Also the uncommitted files right after: if it is still this, undo
    /// puts back exactly what there was before.
    pub fingerprint_after: String,
}

/// Extra copies an operation made before running, to give back on undo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Saved {
    pub stash: Option<String>,
    pub trashed: Vec<(String, PathBuf)>,
}

/// The local refs and the uncommitted files as they are now, to compare
/// with [`OpRecord::fingerprint_after`].
pub async fn current_fingerprint(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<String> {
    let refs = current_refs(git, repo, cancel).await?;
    full_fingerprint(git, repo, &refs, cancel).await
}

async fn full_fingerprint(
    git: &Git,
    repo: &Path,
    refs: &str,
    cancel: &CancellationToken,
) -> Result<String> {
    let files = state_fingerprint(git, repo, cancel).await?;
    Ok(fingerprint(format!("{refs}\0{files}").as_bytes()))
}

/// The state right before an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    head: HeadAt,
    refs: BTreeMap<String, String>,
    index: Option<String>,
}

/// HEAD, and the local branches and tags with their values.
async fn ref_state(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<(HeadAt, BTreeMap<String, String>)> {
    let symbolic = git
        .read(repo)
        .args(["symbolic-ref", "-q", "HEAD"])
        .accept_exit_code(1)
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned();
    let head = if symbolic.is_empty() {
        let oid = git
            .read(repo)
            .args(["rev-parse", "-q", "--verify", "HEAD"])
            .output(cancel)
            .await?
            .stdout_lossy()
            .trim()
            .to_owned();
        HeadAt::Detached { oid }
    } else {
        HeadAt::Branch { name: symbolic }
    };
    // Ref names cannot contain NUL or newlines.
    let listing = git
        .read(repo)
        .args([
            "for-each-ref",
            "--format=%(refname)%00%(objectname)",
            "refs/heads",
            "refs/tags",
        ])
        .output(cancel)
        .await?;
    let refs = listing
        .stdout_lossy()
        .lines()
        .filter_map(|line| line.split_once('\0'))
        .map(|(name, oid)| (name.to_owned(), oid.to_owned()))
        .collect();
    Ok((head, refs))
}

fn refs_fingerprint(head: &HeadAt, refs: &BTreeMap<String, String>) -> String {
    let mut text = match head {
        HeadAt::Branch { name } => format!("ref\0{name}"),
        HeadAt::Detached { oid } => format!("oid\0{oid}"),
    };
    for (name, oid) in refs {
        text.push_str(&format!("\n{name}\0{oid}"));
    }
    fingerprint(text.as_bytes())
}

/// HEAD and the local branches and tags as they are now, to compare with
/// [`OpRecord::refs_after`].
pub async fn current_refs(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<String> {
    let (head, refs) = ref_state(git, repo, cancel).await?;
    Ok(refs_fingerprint(&head, &refs))
}

/// The state right before an operation. Writes the index as a tree object
/// (only an object: no ref, file or index entry changes).
pub async fn snapshot(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<Snapshot> {
    let (head, refs) = ref_state(git, repo, cancel).await?;
    let index = match git.write(repo).args(["write-tree"]).output(cancel).await {
        Ok(out) => Some(out.stdout_lossy().trim().to_owned()),
        Err(GitError::Cancelled) => return Err(GitError::Cancelled),
        // Unmerged entries: there is no tree to give back.
        Err(_) => None,
    };
    Ok(Snapshot { head, refs, index })
}

/// What the operation did since `before`, or `None` if it moved no local
/// ref and left HEAD where it was.
pub async fn record(
    git: &Git,
    repo: &Path,
    before: Snapshot,
    kind: OpKind,
    saved: Saved,
    cancel: &CancellationToken,
) -> Result<Option<OpRecord>> {
    let (head, refs) = ref_state(git, repo, cancel).await?;
    let mut names: Vec<&String> = before.refs.keys().chain(refs.keys()).collect();
    names.sort();
    names.dedup();
    let changes: Vec<RefChange> = names
        .into_iter()
        .filter(|name| before.refs.get(*name) != refs.get(*name))
        .map(|name| RefChange {
            refname: name.clone(),
            before: before.refs.get(name).cloned(),
            after: refs.get(name).cloned(),
        })
        .collect();
    if changes.is_empty() && head == before.head {
        return Ok(None);
    }
    let refs_before = refs_fingerprint(&before.head, &before.refs);
    let refs_after = refs_fingerprint(&head, &refs);
    let remotes_after = git
        .read(repo)
        .args(["for-each-ref", "--format=%(objectname)", "refs/remotes"])
        .output(cancel)
        .await?
        .stdout_lossy()
        .lines()
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let fingerprint_after = full_fingerprint(git, repo, &refs_after, cancel).await?;
    Ok(Some(OpRecord {
        kind,
        refs: changes,
        head_before: before.head,
        head_after: head.clone(),
        index_before: before.index,
        stash: saved.stash,
        trashed: saved.trashed,
        refs_before,
        refs_after,
        remotes_after,
        fingerprint_after,
    }))
}
