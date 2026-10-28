//! Creating, switching, renaming and deleting local branches.
//!
//! Nothing here can lose uncommitted work: switching never forces (git
//! refuses and we explain why), and deleting a branch first shows which
//! commits would be left without one, then deletes it only if it still
//! points where it did when the user confirmed.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::ops::status::working_status;
use crate::{CancellationToken, Git, GitError, Result};

/// How many unreachable commits a deletion preview lists (all are counted).
pub const LISTED_COMMITS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum BranchNameCheck {
    Valid,
    /// Not a valid branch name (see git-check-ref-format).
    Invalid,
    /// A local branch with that name exists.
    Exists,
}

/// A commit that only the branch being deleted reaches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OrphanCommit {
    pub oid: String,
    pub summary: String,
    pub author: String,
    /// Author date, Unix seconds.
    #[cfg_attr(feature = "specta", specta(type = i32))]
    pub time: i64,
}

/// What deleting a branch would leave behind, shown before confirming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BranchDeletion {
    pub name: String,
    /// Where the branch points: to recreate it, and to check nothing moved
    /// between the preview and the deletion.
    pub tip: String,
    /// The tracked remote branch (it is not deleted).
    pub upstream: Option<String>,
    /// Commits no other branch, remote branch or tag contains.
    pub orphaned: u32,
    /// The newest of them (at most [`LISTED_COMMITS`]).
    pub commits: Vec<OrphanCommit>,
}

/// A file whose uncommitted change blocks switching branches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BlockingFile {
    pub path: String,
    pub untracked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum CheckoutOutcome {
    Switched,
    /// git refused because these uncommitted changes would be overwritten;
    /// nothing was touched.
    Blocked {
        files: Vec<BlockingFile>,
        /// How many files have uncommitted changes in total (all of them
        /// would go to the stash).
        changed: u32,
    },
}

/// Message of the stash made by "stash and switch".
pub fn switch_stash_message(branch: &str) -> String {
    format!("GitCRAFT: antes de cambiar a {branch}")
}

fn refname(name: &str) -> String {
    format!("refs/heads/{name}")
}

/// Branch names come from the UI: validated before reaching any command.
async fn validate(git: &Git, repo: &Path, name: &str, cancel: &CancellationToken) -> Result<()> {
    if check_branch_name(git, repo, name, cancel).await? == BranchNameCheck::Invalid {
        return Err(GitError::InvalidBranchName(name.to_owned()));
    }
    Ok(())
}

/// Whether `name` can be used for a new branch.
pub async fn check_branch_name(
    git: &Git,
    repo: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<BranchNameCheck> {
    // `--branch` also rejects names starting with "-"; its output differs
    // from the input for shorthands such as "@{-1}", which are not names.
    let out = git
        .read(repo)
        .args(["check-ref-format", "--branch"])
        .arg(name)
        .accept_exit_code(128)
        .output(cancel)
        .await?;
    if out.exit_code != Some(0) || out.stdout_lossy().trim_end() != name || name == "HEAD" {
        return Ok(BranchNameCheck::Invalid);
    }
    Ok(match branch_tip(git, repo, name, cancel).await? {
        Some(_) => BranchNameCheck::Exists,
        None => BranchNameCheck::Valid,
    })
}

/// The commit a local branch points to, if it exists.
async fn branch_tip(
    git: &Git,
    repo: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify"])
        .arg(format!("{}^{{commit}}", refname(name)))
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let oid = out.stdout_lossy().trim().to_owned();
    Ok((out.exit_code == Some(0) && !oid.is_empty()).then_some(oid))
}

/// Creates branch `name` at `start` (a commit id or a full ref name).
/// `track` sets `start`, a remote branch, as its upstream.
pub async fn create_branch(
    git: &Git,
    repo: &Path,
    name: &str,
    start: &str,
    track: bool,
    cancel: &CancellationToken,
) -> Result<()> {
    match check_branch_name(git, repo, name, cancel).await? {
        BranchNameCheck::Invalid => return Err(GitError::InvalidBranchName(name.to_owned())),
        BranchNameCheck::Exists => return Err(GitError::BranchExists(name.to_owned())),
        BranchNameCheck::Valid => {}
    }
    // Start points are ids or full ref names from the UI: never options.
    if start.is_empty() || start.starts_with('-') {
        return Err(GitError::Parse(format!("invalid start point {start:?}")));
    }
    let mut cmd = git.write(repo).arg("branch");
    cmd = cmd.arg(if track { "--track" } else { "--no-track" });
    cmd.args(["--", name, start]).output(cancel).await?;
    Ok(())
}

/// Renames a local branch (also the current one). Its upstream and reflog
/// go with it.
pub async fn rename_branch(
    git: &Git,
    repo: &Path,
    old: &str,
    new: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    if branch_tip(git, repo, old, cancel).await?.is_none() {
        return Err(GitError::BranchChanged(old.to_owned()));
    }
    match check_branch_name(git, repo, new, cancel).await? {
        BranchNameCheck::Invalid => return Err(GitError::InvalidBranchName(new.to_owned())),
        BranchNameCheck::Exists => return Err(GitError::BranchExists(new.to_owned())),
        BranchNameCheck::Valid => {}
    }
    git.write(repo)
        .args(["branch", "--move", "--", old, new])
        .output(cancel)
        .await?;
    Ok(())
}

/// What deleting `name` would leave without a branch.
pub async fn preview_branch_deletion(
    git: &Git,
    repo: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<BranchDeletion> {
    validate(git, repo, name, cancel).await?;
    let tip = branch_tip(git, repo, name, cancel)
        .await?
        .ok_or_else(|| GitError::BranchChanged(name.to_owned()))?;
    // Every other branch, remote branch and tag. `--exclude` applies to the
    // next `--branches` and takes the name without "refs/heads/"; branch
    // names cannot contain glob characters.
    let others = [
        "--not".to_owned(),
        format!("--exclude={name}"),
        "--branches".to_owned(),
        "--remotes".to_owned(),
        "--tags".to_owned(),
    ];
    let count = git
        .read(repo)
        .args(["rev-list", "--count", &tip])
        .args(&others)
        .output(cancel);
    let listed = git
        .read(repo)
        .args([
            "log",
            "-z",
            "--format=%H%x1f%s%x1f%an%x1f%at",
            &format!("--max-count={LISTED_COMMITS}"),
            &tip,
        ])
        .args(&others)
        .output(cancel);
    let upstream = git
        .read(repo)
        .args(["for-each-ref", "--format=%(upstream:short)"])
        .arg(refname(name))
        .output(cancel);
    let (count, listed, upstream) = tokio::try_join!(count, listed, upstream)?;

    let orphaned = count
        .stdout_lossy()
        .trim()
        .parse()
        .map_err(|_| GitError::Parse("rev-list --count".into()))?;
    let commits = listed
        .stdout_lossy()
        .split('\0')
        .filter(|r| !r.trim().is_empty())
        .map(|record| {
            let fields: Vec<&str> = record.trim_start_matches('\n').split('\u{1f}').collect();
            let [oid, summary, author, time] = fields[..] else {
                return Err(GitError::Parse(format!("log record {record:?}")));
            };
            Ok(OrphanCommit {
                oid: oid.to_owned(),
                summary: summary.to_owned(),
                author: author.to_owned(),
                time: time.trim().parse().unwrap_or(0),
            })
        })
        .collect::<Result<_>>()?;
    let upstream = upstream.stdout_lossy().trim().to_owned();
    Ok(BranchDeletion {
        name: name.to_owned(),
        tip,
        upstream: (!upstream.is_empty()).then_some(upstream),
        orphaned,
        commits,
    })
}

/// Deletes branch `name`, but only if it still points to `expected_tip`
/// (what the confirmation showed). It is deleted even when not merged: the
/// user has seen exactly which commits that leaves without a branch.
pub async fn delete_branch(
    git: &Git,
    repo: &Path,
    name: &str,
    expected_tip: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    validate(git, repo, name, cancel).await?;
    if branch_tip(git, repo, name, cancel).await?.as_deref() != Some(expected_tip) {
        return Err(GitError::BranchChanged(name.to_owned()));
    }
    git.write(repo)
        .args(["branch", "--delete", "--force", "--", name])
        .output(cancel)
        .await?;
    Ok(())
}

/// Switches to local branch `name`. Never forces: when uncommitted changes
/// would be overwritten git refuses without touching anything, and the
/// outcome lists the files involved.
pub async fn checkout_branch(
    git: &Git,
    repo: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<CheckoutOutcome> {
    validate(git, repo, name, cancel).await?;
    let result = git
        .write(repo)
        .args(["switch", "--quiet", "--no-guess", "--", name])
        .output(cancel)
        .await;
    let Err(error) = result else {
        return Ok(CheckoutOutcome::Switched);
    };
    match blocking_files(git, repo, name, cancel).await {
        Ok((files, changed)) if !files.is_empty() => {
            Ok(CheckoutOutcome::Blocked { files, changed })
        }
        // Some other reason (e.g. a merge in progress): git's own message.
        _ => Err(error),
    }
}

/// Uncommitted changes in files that differ between HEAD and `branch`:
/// the ones git would have to overwrite. Also the total number of changed
/// files.
async fn blocking_files(
    git: &Git,
    repo: &Path,
    branch: &str,
    cancel: &CancellationToken,
) -> Result<(Vec<BlockingFile>, u32)> {
    let differing = git
        .read(repo)
        .args(["diff", "--name-only", "-z", "--no-renames", "HEAD"])
        .arg(refname(branch))
        .arg("--")
        .output(cancel);
    let status = working_status(git, repo, cancel);
    let (differing, status) = tokio::try_join!(differing, status)?;
    let differing = differing.stdout_lossy();
    let differing: BTreeSet<&str> = differing.split('\0').filter(|p| !p.is_empty()).collect();

    let mut changed = BTreeSet::new();
    let mut files = Vec::new();
    for entry in status.staged.iter().chain(&status.unstaged) {
        for path in std::iter::once(&entry.path).chain(&entry.old_path) {
            changed.insert(path.clone());
        }
    }
    for path in &changed {
        if differing.contains(path.as_str()) {
            files.push(BlockingFile {
                path: path.clone(),
                untracked: false,
            });
        }
    }
    for entry in &status.untracked {
        changed.insert(entry.path.clone());
        // An untracked directory ("dir/") blocks if the branch has files in it.
        let blocks = if entry.path.ends_with('/') {
            differing.iter().any(|d| d.starts_with(entry.path.as_str()))
        } else {
            differing.contains(entry.path.as_str())
        };
        if blocks {
            files.push(BlockingFile {
                path: entry.path.clone(),
                untracked: true,
            });
        }
    }
    let changed = u32::try_from(changed.len()).unwrap_or(u32::MAX);
    Ok((files, changed))
}

/// "Stash and switch": saves every uncommitted change (and untracked files
/// when `include_untracked`) in a stash entry named after the target
/// branch, then switches. The stash entry is never dropped automatically.
pub async fn stash_and_checkout(
    git: &Git,
    repo: &Path,
    name: &str,
    include_untracked: bool,
    cancel: &CancellationToken,
) -> Result<CheckoutOutcome> {
    validate(git, repo, name, cancel).await?;
    let mut cmd = git.write(repo).args(["stash", "push", "--quiet"]);
    if include_untracked {
        cmd = cmd.arg("--include-untracked");
    }
    cmd.arg("--message")
        .arg(switch_stash_message(name))
        .output(cancel)
        .await?;
    checkout_branch(git, repo, name, cancel).await
}
