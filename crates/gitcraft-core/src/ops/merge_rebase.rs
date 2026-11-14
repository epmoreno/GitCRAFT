//! Merging one branch into another, or rebasing it onto another (task 8):
//! what dropping the branch `source` on the branch `target` would do.
//!
//! - **Merge** of `source` into `target`: `target` moves (a fast-forward if
//!   it has no commits of its own, otherwise a merge commit).
//! - **Rebase** of `source` onto `target`: `source` moves; its own commits
//!   are replayed on top of `target`, with new ids.
//!
//! The branch that moves must be the current one, except a fast-forward of
//! another local branch (only its ref moves; the working tree is not
//! touched). Remote branches can be the source, never the branch that
//! moves. Like the pull and the cherry-pick, nothing is touched before the
//! user accepts: the previews here are read-only (predicting a rebase writes
//! loose objects, never refs).

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ops::branch::BlockingFile;
use crate::ops::in_progress::{CommitRef, commit_ref};
use crate::ops::reset::{remote_branches_containing, state_fingerprint};
use crate::ops::stash::create_stash;
use crate::ops::status::{in_progress, working_status};
use crate::parse::diff::fingerprint;
use crate::parse::status::WorkingStatus;
use crate::{CancellationToken, Git, GitError, Result};

/// Commits listed at most; `commits_total` says how many there are.
const MAX_LISTED: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum MoveKind {
    /// `source` is merged into `target`: `target` moves.
    Merge,
    /// `source` is replayed onto `target`: `source` moves.
    Rebase,
}

/// A branch taking part, as it was when the preview was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BranchTip {
    /// Full name: `refs/heads/…` or `refs/remotes/…`.
    pub refname: String,
    /// Short name, as the sidebar shows it (`main`, `origin/main`).
    pub name: String,
    pub remote: bool,
    pub oid: String,
    pub short: String,
}

/// What would happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum MoveCase {
    /// Nothing to do: the branch that would move already has everything.
    UpToDate,
    /// The branch that moves only advances: no new commit, nothing replayed.
    FastForward,
    /// A merge commit with two parents.
    Merge,
    /// Commits replayed with new ids.
    Rebase,
}

/// Why it cannot be done at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum MoveRefusal {
    /// A merge, rebase… is waiting for the user.
    InProgress,
    /// HEAD is not on a branch (or there are no commits yet).
    Detached,
    /// Both are the same branch.
    SameBranch,
    /// The branch that would move is a remote branch.
    RemoteMoves,
    /// The branch that would move is not the current one (and this is not
    /// a fast-forward of a local branch).
    NotCurrent,
    /// Fast-forward of a branch checked out in another worktree: moving its
    /// ref would leave that worktree out of step.
    CheckedOutElsewhere,
    /// The two branches have no commit in common.
    Unrelated,
    /// Rebase: the commits to replay include merges, which git would
    /// flatten (the merges would be lost).
    MergesInRange,
}

/// Why uncommitted changes are in the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum MoveBlock {
    /// A merge commit needs nothing staged, even in other files.
    StagedChanges,
    /// Changes (or untracked files) in files the operation changes.
    Overwrite,
    /// A rebase needs no uncommitted changes in tracked files at all.
    RebaseNeedsClean,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MoveCommit {
    pub commit: CommitRef,
    /// Rebase: a remote branch has it, so rewriting it will need a forced
    /// push, which affects whoever already has it. Always false in a merge.
    pub published: bool,
}

/// Where a rebase would stop with conflicts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RebaseStop {
    /// 1-based, among the commits to replay.
    pub index: u32,
    pub commit: CommitRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MovePreview {
    pub kind: MoveKind,
    pub source: BranchTip,
    pub target: BranchTip,
    /// The current branch (`None`: detached HEAD or no commits yet).
    pub current: Option<String>,
    /// The short name of the branch that moves.
    pub moving: String,
    pub refused: Option<MoveRefusal>,
    pub case: MoveCase,
    /// - Fast-forward: the commits the branch gains, newest first.
    /// - Merge: the commits that come in, newest first.
    /// - Rebase: the commits replayed, in the order git replays them.
    ///
    /// At most [`MAX_LISTED`].
    pub commits: Vec<MoveCommit>,
    pub commits_total: u32,
    /// Rebase: commits git leaves out because `target` already has the same
    /// change.
    pub skipped: u32,
    /// Rebase: remote branches that have some of the replayed commits.
    pub published_in: Vec<String>,
    /// Uncommitted changes in the way.
    pub blocked: Option<MoveBlock>,
    pub blocking: Vec<BlockingFile>,
    /// Files that would stop with conflicts (in a rebase, at `stops_at`;
    /// after that commit nothing is known).
    pub conflicts: Vec<String>,
    pub stops_at: Option<RebaseStop>,
    /// The conflicts come from git's own merge (git ≥ 2.40); otherwise they
    /// are an estimate (files changed on both sides).
    pub exact: bool,
    /// Merge into the current branch (with a merge commit, or a
    /// fast-forward where the user may ask for one): the message git would
    /// write (`git fmt-merge-msg`).
    pub message: Option<String>,
    /// The repository as the preview saw it (HEAD, the uncommitted files and
    /// both branches): nothing is done if it changed.
    pub fingerprint: String,
}

fn split_z(stdout: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `refname`, which comes from the UI: a local or remote branch that exists.
async fn branch_tip(
    git: &Git,
    repo: &Path,
    refname: &str,
    cancel: &CancellationToken,
) -> Result<BranchTip> {
    let (name, remote) = if let Some(name) = refname.strip_prefix("refs/heads/") {
        (name, false)
    } else if let Some(name) = refname.strip_prefix("refs/remotes/") {
        (name, true)
    } else {
        return Err(GitError::InvalidBranchName(refname.to_owned()));
    };
    let valid = git
        .read(repo)
        .args(["check-ref-format", refname])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    // git itself never creates a branch starting with "-".
    if name.is_empty()
        || name.starts_with('-')
        || valid.exit_code != Some(0)
        || (remote && name.ends_with("/HEAD"))
    {
        return Err(GitError::InvalidBranchName(refname.to_owned()));
    }
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "--end-of-options"])
        .arg(format!("{refname}^{{commit}}"))
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let oid = out.stdout_lossy().trim().to_owned();
    if oid.is_empty() {
        return Err(GitError::BranchChanged(name.to_owned()));
    }
    let short = git
        .read(repo)
        .args(["rev-parse", "--short", &oid])
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned();
    Ok(BranchTip {
        refname: refname.to_owned(),
        name: name.to_owned(),
        remote,
        oid,
        short,
    })
}

async fn merge_base(
    git: &Git,
    repo: &Path,
    a: &str,
    b: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(["merge-base", a, b])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let base = out.stdout_lossy().trim().to_owned();
    Ok((!base.is_empty()).then_some(base))
}

async fn lines(
    git: &Git,
    repo: &Path,
    args: &[&str],
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    let out = git.read(repo).args(args).output(cancel).await?;
    Ok(out.stdout_lossy().lines().map(str::to_owned).collect())
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
    Ok(split_z(&out.stdout).into_iter().collect())
}

/// Id, short id and subject of each commit in `args` (a `git log` range or
/// list), at most [`MAX_LISTED`].
async fn commit_refs(
    git: &Git,
    repo: &Path,
    args: &[&str],
    cancel: &CancellationToken,
) -> Result<Vec<CommitRef>> {
    let out = git
        .read(repo)
        .args([
            "log",
            "-z",
            "--no-show-signature",
            "--format=%H%x1f%h%x1f%s",
        ])
        .arg(format!("--max-count={MAX_LISTED}"))
        .args(args)
        .output(cancel)
        .await?;
    split_z(&out.stdout)
        .into_iter()
        .map(|record| {
            let mut parts = record.trim_start_matches('\n').splitn(3, '\u{1f}');
            match (parts.next(), parts.next(), parts.next()) {
                (Some(oid), Some(short), Some(subject)) => Ok(CommitRef {
                    oid: oid.to_owned(),
                    short: short.to_owned(),
                    subject: subject.to_owned(),
                }),
                _ => Err(GitError::Parse(format!("log record: {record:?}"))),
            }
        })
        .collect()
}

/// Whether a worktree has `branch` (a local branch name) checked out.
async fn checked_out_somewhere(
    git: &Git,
    repo: &Path,
    branch: &str,
    cancel: &CancellationToken,
) -> Result<bool> {
    let out = git
        .read(repo)
        .args(["worktree", "list", "--porcelain", "-z"])
        .output(cancel)
        .await?;
    let wanted = format!("branch refs/heads/{branch}");
    Ok(split_z(&out.stdout).contains(&wanted))
}

/// The uncommitted changes and untracked files in the way of changing the
/// files in `touched`; with `all_tracked`, any change in a tracked file is.
fn in_the_way(
    status: &WorkingStatus,
    touched: &BTreeSet<String>,
    all_tracked: bool,
) -> Vec<BlockingFile> {
    let mut tracked = BTreeSet::new();
    for entry in status.staged.iter().chain(&status.unstaged) {
        tracked.insert(entry.path.clone());
        tracked.extend(entry.old_path.clone());
    }
    let mut files: Vec<BlockingFile> = tracked
        .into_iter()
        .filter(|p| all_tracked || touched.contains(p))
        .map(|path| BlockingFile {
            path,
            untracked: false,
        })
        .collect();
    files.extend(
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
    files
}

/// Runs `merge-tree --write-tree` with `extra` (and `-z`): `Ok(Ok(tree))`
/// if clean, `Ok(Err(files))` with conflicts, `Err` if this git cannot.
async fn merge_tree(
    git: &Git,
    repo: &Path,
    extra: &[String],
    cancel: &CancellationToken,
) -> Result<std::result::Result<String, Vec<String>>> {
    let out = git
        .read(repo)
        .args([
            "merge-tree",
            "--write-tree",
            "--name-only",
            "--no-messages",
            "-z",
        ])
        .args(extra)
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut fields = stdout.split('\0');
    let tree = fields.next().unwrap_or_default().trim().to_owned();
    if out.exit_code == Some(0) {
        return Ok(Ok(tree));
    }
    let files: BTreeSet<String> = fields
        .take_while(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(Err(files.into_iter().collect()))
}

/// Files changed on both sides since `base`: the estimate for older git.
async fn both_changed(
    git: &Git,
    repo: &Path,
    base: &str,
    ours: &str,
    theirs: &str,
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    let ours = changed(git, repo, base, ours, cancel).await?;
    let theirs = changed(git, repo, base, theirs, cancel).await?;
    Ok(ours.intersection(&theirs).cloned().collect())
}

/// The message `git merge` would write merging `source` into the current
/// branch.
async fn merge_message(
    git: &Git,
    repo: &Path,
    source: &BranchTip,
    cancel: &CancellationToken,
) -> Result<String> {
    let what = if source.remote {
        "remote-tracking branch"
    } else {
        "branch"
    };
    // The same line `git merge` hands to fmt-merge-msg for a local merge.
    let line = format!("{}\t\t{what} '{}' of .\n", source.oid, source.name);
    let out = git
        .read(repo)
        .arg("fmt-merge-msg")
        .stdin(line.into_bytes())
        .output(cancel)
        .await?;
    Ok(out.stdout_lossy().trim_end().to_owned())
}

/// What merging the branch `source` into the branch `target` would do
/// (both full ref names). Read-only.
pub async fn merge_preview(
    git: &Git,
    repo: &Path,
    source: &str,
    target: &str,
    cancel: &CancellationToken,
) -> Result<MovePreview> {
    preview(git, repo, MoveKind::Merge, source, target, cancel).await
}

/// What rebasing the branch `source` onto the branch `target` would do
/// (both full ref names). Read-only, except for loose objects written to
/// predict the conflicts.
pub async fn rebase_preview(
    git: &Git,
    repo: &Path,
    source: &str,
    target: &str,
    cancel: &CancellationToken,
) -> Result<MovePreview> {
    preview(git, repo, MoveKind::Rebase, source, target, cancel).await
}

async fn preview(
    git: &Git,
    repo: &Path,
    kind: MoveKind,
    source: &str,
    target: &str,
    cancel: &CancellationToken,
) -> Result<MovePreview> {
    let source = branch_tip(git, repo, source, cancel).await?;
    let target = branch_tip(git, repo, target, cancel).await?;
    let moving_tip = match kind {
        MoveKind::Merge => &target,
        MoveKind::Rebase => &source,
    };
    let current = git
        .read(repo)
        .args(["symbolic-ref", "-q", "--short", "HEAD"])
        .accept_exit_code(1)
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned();
    let has_head = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "HEAD^{commit}"])
        .accept_exit_code(1)
        .output(cancel)
        .await?
        .exit_code
        == Some(0);
    let current = (has_head && !current.is_empty()).then_some(current);
    let state = state_fingerprint(git, repo, cancel).await?;
    let mut preview = MovePreview {
        kind,
        current: current.clone(),
        moving: moving_tip.name.clone(),
        refused: None,
        case: MoveCase::UpToDate,
        commits: Vec::new(),
        commits_total: 0,
        skipped: 0,
        published_in: Vec::new(),
        blocked: None,
        blocking: Vec::new(),
        conflicts: Vec::new(),
        stops_at: None,
        exact: true,
        message: None,
        fingerprint: fingerprint(format!("{state}\0{}\0{}", source.oid, target.oid).as_bytes()),
        source: source.clone(),
        target: target.clone(),
    };
    let refuse = |mut p: MovePreview, why| {
        p.refused = Some(why);
        Ok(p)
    };
    if in_progress(git, repo, cancel).await?.is_some() {
        return refuse(preview, MoveRefusal::InProgress);
    }
    let Some(current) = current else {
        return refuse(preview, MoveRefusal::Detached);
    };
    if source.refname == target.refname {
        return refuse(preview, MoveRefusal::SameBranch);
    }
    let moving = moving_tip.clone();
    if moving.remote {
        return refuse(preview, MoveRefusal::RemoteMoves);
    }
    let Some(base) = merge_base(git, repo, &source.oid, &target.oid, cancel).await? else {
        return refuse(preview, MoveRefusal::Unrelated);
    };

    // What happens, and to which commits.
    let (up_to_date, fast_forward) = match kind {
        // Merge: target already has source / target is behind source.
        MoveKind::Merge => (base == source.oid, base == target.oid),
        // Rebase: source already sits on target / source is behind target.
        MoveKind::Rebase => (base == target.oid, base == source.oid),
    };
    let ahead = format!("{}..{}", target.oid, source.oid);
    if up_to_date {
        return Ok(preview);
    }
    // The commit the moving branch ends at in a fast-forward.
    let mut new_tip = None;
    let mut picks: Vec<String> = Vec::new();
    if fast_forward {
        preview.case = MoveCase::FastForward;
        let other = match kind {
            MoveKind::Merge => &source,
            MoveKind::Rebase => &target,
        };
        let gained = format!("{}..{}", moving.oid, other.oid);
        let total = lines(git, repo, &["rev-list", "--count", &gained], cancel).await?;
        preview.commits_total = total.first().and_then(|n| n.parse().ok()).unwrap_or(0);
        preview.commits = commit_refs(git, repo, &[&gained], cancel)
            .await?
            .into_iter()
            .map(|commit| MoveCommit {
                commit,
                published: false,
            })
            .collect();
        new_tip = Some(other.oid.clone());
    } else if kind == MoveKind::Merge {
        preview.case = MoveCase::Merge;
        let total = lines(git, repo, &["rev-list", "--count", &ahead], cancel).await?;
        preview.commits_total = total.first().and_then(|n| n.parse().ok()).unwrap_or(0);
        preview.commits = commit_refs(git, repo, &[&ahead], cancel)
            .await?
            .into_iter()
            .map(|commit| MoveCommit {
                commit,
                published: false,
            })
            .collect();
    } else {
        preview.case = MoveCase::Rebase;
        let merges = lines(git, repo, &["rev-list", "--merges", &ahead], cancel).await?;
        if !merges.is_empty() {
            return refuse(preview, MoveRefusal::MergesInRange);
        }
        // Exactly the commits git replays: those whose change target does
        // not already have.
        let symmetric = format!("{}...{}", target.oid, source.oid);
        picks = lines(
            git,
            repo,
            &[
                "rev-list",
                "--reverse",
                "--topo-order",
                "--right-only",
                "--cherry-pick",
                "--no-merges",
                &symmetric,
            ],
            cancel,
        )
        .await?;
        let all = lines(git, repo, &["rev-list", &ahead], cancel).await?;
        preview.skipped = u32::try_from(all.len() - picks.len().min(all.len())).unwrap_or(0);
        preview.commits_total = u32::try_from(picks.len()).unwrap_or(u32::MAX);
        let not_published: BTreeSet<String> = lines(
            git,
            repo,
            &["rev-list", &ahead, "--not", "--remotes"],
            cancel,
        )
        .await?
        .into_iter()
        .collect();
        let listed: Vec<&str> = picks.iter().take(MAX_LISTED).map(String::as_str).collect();
        if !listed.is_empty() {
            let mut args = vec!["--no-walk=unsorted"];
            args.extend(listed);
            preview.commits = commit_refs(git, repo, &args, cancel)
                .await?
                .into_iter()
                .map(|commit| MoveCommit {
                    published: !not_published.contains(&commit.oid),
                    commit,
                })
                .collect();
        }
        if let Some(oldest) = picks.iter().find(|oid| !not_published.contains(*oid)) {
            preview.published_in = remote_branches_containing(git, repo, oldest, cancel).await?;
        }
    }

    // Only the current branch moves, except a fast-forward of another local
    // branch (its ref alone).
    if moving.name != current {
        if !fast_forward {
            return refuse(preview, MoveRefusal::NotCurrent);
        }
        if checked_out_somewhere(git, repo, &moving.name, cancel).await? {
            return refuse(preview, MoveRefusal::CheckedOutElsewhere);
        }
        return Ok(preview);
    }

    // Uncommitted changes in the way.
    let status = working_status(git, repo, cancel).await?;
    let blocked = match (&new_tip, kind) {
        (Some(tip), _) => {
            let touched = changed(git, repo, "HEAD", tip, cancel).await?;
            let files = in_the_way(&status, &touched, false);
            (!files.is_empty()).then_some((files, MoveBlock::Overwrite))
        }
        (None, MoveKind::Merge) if !status.staged.is_empty() => Some((
            status
                .staged
                .iter()
                .map(|e| BlockingFile {
                    path: e.path.clone(),
                    untracked: false,
                })
                .collect(),
            MoveBlock::StagedChanges,
        )),
        (None, MoveKind::Merge) => {
            let touched = changed(git, repo, &base, &source.oid, cancel).await?;
            let files = in_the_way(&status, &touched, false);
            (!files.is_empty()).then_some((files, MoveBlock::Overwrite))
        }
        (None, MoveKind::Rebase) => {
            // The working tree goes to target first, then each commit.
            let mut touched = changed(git, repo, "HEAD", &target.oid, cancel).await?;
            for pick in &picks {
                touched.extend(changed(git, repo, &format!("{pick}^"), pick, cancel).await?);
            }
            let dirty = !status.staged.is_empty() || !status.unstaged.is_empty();
            let files = in_the_way(&status, &touched, dirty);
            let reason = if dirty {
                MoveBlock::RebaseNeedsClean
            } else {
                MoveBlock::Overwrite
            };
            (!files.is_empty()).then_some((files, reason))
        }
    };
    if let Some((files, reason)) = blocked {
        preview.blocked = Some(reason);
        preview.blocking = files;
    }

    // Predicted conflicts.
    match preview.case {
        MoveCase::FastForward if kind == MoveKind::Merge => {
            preview.message = Some(merge_message(git, repo, &source, cancel).await?);
        }
        MoveCase::Merge => {
            preview.message = Some(merge_message(git, repo, &source, cancel).await?);
            let args = ["HEAD".to_owned(), source.oid.clone()];
            match merge_tree(git, repo, &args, cancel).await {
                Ok(Ok(_)) => {}
                Ok(Err(files)) => preview.conflicts = files,
                Err(_) => {
                    preview.conflicts =
                        both_changed(git, repo, &base, "HEAD", &source.oid, cancel).await?;
                    preview.exact = false;
                }
            }
        }
        MoveCase::Rebase => match predict_rebase(git, repo, &target.oid, &picks, cancel).await {
            Ok(Some((index, files))) => {
                let oid = &picks[index];
                preview.stops_at = Some(RebaseStop {
                    index: u32::try_from(index + 1).unwrap_or(u32::MAX),
                    commit: commit_ref(git, repo, oid, cancel).await?,
                });
                preview.conflicts = files;
            }
            Ok(None) => {}
            Err(GitError::Cancelled) => return Err(GitError::Cancelled),
            Err(_) => {
                preview.conflicts =
                    both_changed(git, repo, &base, &target.oid, &source.oid, cancel).await?;
                preview.exact = false;
            }
        },
        MoveCase::UpToDate | MoveCase::FastForward => {}
    }
    Ok(preview)
}

/// Replays `picks` (oldest first) onto `onto` as git's rebase would, in
/// objects only: returns the first one that conflicts and its files.
async fn predict_rebase(
    git: &Git,
    repo: &Path,
    onto: &str,
    picks: &[String],
    cancel: &CancellationToken,
) -> Result<Option<(usize, Vec<String>)>> {
    let mut onto = onto.to_owned();
    for (index, pick) in picks.iter().enumerate() {
        let args = [format!("--merge-base={pick}^"), onto.clone(), pick.clone()];
        let tree = match merge_tree(git, repo, &args, cancel).await? {
            Ok(tree) => tree,
            Err(files) => return Ok(Some((index, files))),
        };
        // A commit whose change is already there becomes empty: git drops it.
        let onto_tree = git
            .read(repo)
            .args(["rev-parse", &format!("{onto}^{{tree}}")])
            .output(cancel)
            .await?
            .stdout_lossy()
            .trim()
            .to_owned();
        if tree == onto_tree {
            continue;
        }
        // A throwaway commit (no ref points at it; git's gc removes it), so
        // the next commit is replayed on top of this one.
        let out = git
            .read(repo)
            .args([
                "commit-tree",
                "--no-gpg-sign",
                &tree,
                "-p",
                &onto,
                "-m",
                "preview",
            ])
            .env("GIT_AUTHOR_NAME", "GitCRAFT")
            .env("GIT_AUTHOR_EMAIL", "preview@gitcraft.invalid")
            .env("GIT_COMMITTER_NAME", "GitCRAFT")
            .env("GIT_COMMITTER_EMAIL", "preview@gitcraft.invalid")
            .output(cancel)
            .await?;
        onto = out.stdout_lossy().trim().to_owned();
    }
    Ok(None)
}

/// What the user accepted in the merge dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MergeRequest {
    /// Full ref names, as in the preview.
    pub source: String,
    pub target: String,
    /// The preview's fingerprint: nothing is done if the repository changed.
    pub expected: String,
    /// Create a merge commit even when a fast-forward is possible.
    pub no_ff: bool,
    /// The merge commit's message (`None` or blank: git's own).
    pub message: Option<String>,
    /// Stash the changes in the way first.
    pub save_changes: bool,
    /// Go on even if it will stop with conflicts.
    pub allow_conflicts: bool,
}

/// In every outcome, `saved` is the stash entry (commit id) holding the
/// changes "save my changes" put aside, so the app can offer them back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum MoveOutcome {
    /// Done: `head` is where the branch that moved is now.
    Done {
        head: String,
        fast_forward: bool,
        saved: Option<String>,
    },
    /// Started at the user's request and stopped on conflicts in these
    /// files: a merge or rebase is waiting (task 9's banner and view).
    Conflicted {
        files: Vec<String>,
        saved: Option<String>,
    },
    /// Nothing was done; the preview says why. `changed`: the repository
    /// is no longer as the preview the user accepted saw it.
    NotDone {
        preview: Box<MovePreview>,
        changed: bool,
        saved: Option<String>,
    },
}

/// Everything needed to undo a merge or rebase that ended; kept by the
/// app, not shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MoveRecord {
    /// The local branch that moved.
    pub branch: String,
    /// It was the current branch (otherwise only its ref moved).
    pub current: bool,
    /// Where it was, and where it ended.
    pub previous: String,
    pub after: String,
    /// Current branch: the index (as a tree) right before the operation,
    /// with what was staged.
    pub index_before: Option<String>,
    /// The stash entry with the changes saved before it, if any.
    pub stash: Option<String>,
    /// Current branch: the state right after; undo only if it is still
    /// this.
    pub fingerprint_after: String,
}

/// Message of the stash made by "save my changes, then merge".
pub fn merge_stash_message(source: &str) -> String {
    format!("GitCRAFT: antes del merge de {source}")
}

async fn ref_value(
    git: &Git,
    repo: &Path,
    refname: &str,
    cancel: &CancellationToken,
) -> Result<String> {
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "--end-of-options", refname])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    Ok(out.stdout_lossy().trim().to_owned())
}

/// Exit code 1 means "stopped on conflicts" only if something is waiting;
/// otherwise git refused (a hook, for instance): that is an error.
fn stopped_or_failed(out: &crate::GitOutput, status: &WorkingStatus, command: &str) -> Result<()> {
    if out.exit_code == Some(1) && status.in_progress.is_none() && status.conflicted.is_empty() {
        return Err(GitError::Failed {
            args: vec![command.to_owned()],
            exit_code: out.exit_code,
            stderr: out.stderr.clone(),
            stdout_tail: String::from_utf8_lossy(&out.stdout).into_owned(),
        });
    }
    Ok(())
}

/// Merges `source` into `target` as the user accepted it. It checks
/// everything again and does nothing if the repository changed since the
/// preview, if it is refused, if changes are in the way (unless
/// `save_changes`: they are stashed first, in an entry that is never
/// dropped automatically) or if it would conflict (unless
/// `allow_conflicts`). Returns how to undo it when it is done.
pub async fn merge(
    git: &Git,
    repo: &Path,
    request: &MergeRequest,
    cancel: &CancellationToken,
) -> Result<(MoveOutcome, Option<MoveRecord>)> {
    let not_done = |preview: MovePreview, changed, saved| {
        Ok((
            MoveOutcome::NotDone {
                preview: Box::new(preview),
                changed,
                saved,
            },
            None,
        ))
    };
    let mut preview = merge_preview(git, repo, &request.source, &request.target, cancel).await?;
    if preview.fingerprint != request.expected {
        return not_done(preview, true, None);
    }
    if preview.refused.is_some() || preview.case == MoveCase::UpToDate {
        return not_done(preview, false, None);
    }
    let current = preview.current.as_deref() == Some(preview.moving.as_str());
    let mut saved = None;
    if preview.blocked.is_some() {
        if !request.save_changes {
            return not_done(preview, false, saved);
        }
        let message = merge_stash_message(&preview.source.name);
        saved = Some(create_stash(git, repo, &message, true, cancel).await?);
        preview = merge_preview(git, repo, &request.source, &request.target, cancel).await?;
        if preview.refused.is_some() || preview.blocked.is_some() {
            return not_done(preview, false, saved);
        }
    }
    let commit = preview.case == MoveCase::Merge || request.no_ff;
    if commit && !current {
        // A merge commit is only made on the current branch.
        return not_done(preview, false, saved);
    }
    if commit && !preview.conflicts.is_empty() && !request.allow_conflicts {
        return not_done(preview, false, saved);
    }
    let previous = preview.target.oid.clone();
    let target_ref = preview.target.refname.clone();

    if !current {
        // Fast-forward of another local branch: its ref alone, and only if
        // it is still where the preview saw it.
        git.write(repo)
            .args(["update-ref", "-m"])
            .arg(format!("merge {}: Fast-forward", preview.source.name))
            .args([&target_ref, &preview.source.oid, &previous])
            .output(cancel)
            .await?;
        let record = MoveRecord {
            branch: preview.target.name.clone(),
            current: false,
            previous,
            after: preview.source.oid.clone(),
            index_before: None,
            stash: saved.clone(),
            fingerprint_after: String::new(),
        };
        return Ok((
            MoveOutcome::Done {
                head: record.after.clone(),
                fast_forward: true,
                saved,
            },
            Some(record),
        ));
    }

    let index_before = git
        .write(repo)
        .arg("write-tree")
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned();
    let mut cmd = git.write(repo).arg("merge");
    if commit {
        let message = request
            .message
            .as_deref()
            .filter(|m| !m.trim().is_empty())
            .or(preview.message.as_deref())
            .unwrap_or_default()
            .to_owned();
        cmd = cmd.args(["--no-ff", "--no-edit", "-m", &message]);
    } else {
        cmd = cmd.arg("--ff-only");
    }
    let out = cmd
        .args(["--end-of-options", &preview.source.refname])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let status = working_status(git, repo, cancel).await?;
    stopped_or_failed(&out, &status, "merge")?;
    if out.exit_code == Some(1) || !status.conflicted.is_empty() || status.in_progress.is_some() {
        return Ok((
            MoveOutcome::Conflicted {
                files: status.conflicted.into_iter().map(|c| c.path).collect(),
                saved,
            },
            None,
        ));
    }
    let head = ref_value(git, repo, &target_ref, cancel).await?;
    let record = MoveRecord {
        branch: preview.target.name.clone(),
        current: true,
        previous,
        after: head.clone(),
        index_before: Some(index_before),
        stash: saved.clone(),
        fingerprint_after: state_fingerprint(git, repo, cancel).await?,
    };
    Ok((
        MoveOutcome::Done {
            head,
            fast_forward: !commit,
            saved,
        },
        Some(record),
    ))
}

/// Puts the branch (and, if it was the current one, the index and the
/// working tree) back exactly as before the merge or rebase. Refuses,
/// changing nothing, if anything changed since (`GitError::UndoConflict`).
pub async fn undo_move(
    git: &Git,
    repo: &Path,
    record: &MoveRecord,
    cancel: &CancellationToken,
) -> Result<()> {
    let refname = format!("refs/heads/{}", record.branch);
    let conflict = || GitError::UndoConflict(record.branch.clone());
    if ref_value(git, repo, &refname, cancel).await? != record.after
        || in_progress(git, repo, cancel).await?.is_some()
    {
        return Err(conflict());
    }
    let current = git
        .read(repo)
        .args(["symbolic-ref", "-q", "--short", "HEAD"])
        .accept_exit_code(1)
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned();
    if !record.current {
        if current == record.branch {
            return Err(conflict());
        }
        git.write(repo)
            .args(["update-ref", "-m", "GitCRAFT: undo", &refname])
            .args([&record.previous, &record.after])
            .output(cancel)
            .await?;
        return Ok(());
    }
    if current != record.branch
        || state_fingerprint(git, repo, cancel).await? != record.fingerprint_after
    {
        return Err(conflict());
    }
    // Files that differ between the two commits go back; local changes in
    // other files stay (git refuses if one of them has local changes).
    git.write(repo)
        .args(["reset", "--quiet", "--keep", &record.previous])
        .output(cancel)
        .await?;
    if let Some(tree) = &record.index_before {
        // --keep resets the index: what was staged comes back.
        git.write(repo)
            .args(["read-tree", tree])
            .output(cancel)
            .await?;
        git.write(repo)
            .args(["update-index", "-q", "--refresh"])
            .accept_exit_code(1)
            .output(cancel)
            .await?;
    }
    if let Some(stash) = &record.stash {
        git.write(repo)
            .args(["stash", "apply", "--index", "--quiet", stash])
            .output(cancel)
            .await?;
    }
    Ok(())
}

/// What the user accepted in the rebase dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RebaseRequest {
    /// Full ref names, as in the preview.
    pub source: String,
    pub target: String,
    /// The preview's fingerprint: nothing is done if the repository changed.
    pub expected: String,
    /// Stash the changes in the way first.
    pub save_changes: bool,
    /// Go on even if it will stop with conflicts.
    pub allow_conflicts: bool,
}

/// Message of the stash made by "save my changes, then rebase".
pub fn rebase_stash_message(source: &str) -> String {
    format!("GitCRAFT: antes del rebase de {source}")
}

/// Rebases `source` onto `target` as the user accepted it, with the same
/// checks as [`merge`]. The user's configuration cannot make it do more
/// than the preview said: no other branch is moved (`rebase.updateRefs`),
/// no commit is squashed (`rebase.autoSquash`) and merges are not kept
/// (`rebase.rebaseMerges`; the preview refuses them anyway). Returns how to
/// undo it when it is done.
pub async fn rebase(
    git: &Git,
    repo: &Path,
    request: &RebaseRequest,
    cancel: &CancellationToken,
) -> Result<(MoveOutcome, Option<MoveRecord>)> {
    let not_done = |preview: MovePreview, changed, saved| {
        Ok((
            MoveOutcome::NotDone {
                preview: Box::new(preview),
                changed,
                saved,
            },
            None,
        ))
    };
    let mut preview = rebase_preview(git, repo, &request.source, &request.target, cancel).await?;
    if preview.fingerprint != request.expected {
        return not_done(preview, true, None);
    }
    if preview.refused.is_some() || preview.case == MoveCase::UpToDate {
        return not_done(preview, false, None);
    }
    let current = preview.current.as_deref() == Some(preview.moving.as_str());
    let mut saved = None;
    if preview.blocked.is_some() {
        if !request.save_changes {
            return not_done(preview, false, saved);
        }
        let message = rebase_stash_message(&preview.source.name);
        saved = Some(create_stash(git, repo, &message, true, cancel).await?);
        preview = rebase_preview(git, repo, &request.source, &request.target, cancel).await?;
        if preview.refused.is_some() || preview.blocked.is_some() {
            return not_done(preview, false, saved);
        }
    }
    if !preview.conflicts.is_empty() && !request.allow_conflicts {
        return not_done(preview, false, saved);
    }
    let previous = preview.source.oid.clone();
    let source_ref = preview.source.refname.clone();
    let fast_forward = preview.case == MoveCase::FastForward;

    if !current {
        // Fast-forward of another local branch: its ref alone, and only if
        // it is still where the preview saw it.
        git.write(repo)
            .args(["update-ref", "-m"])
            .arg(format!("rebase onto {}: Fast-forward", preview.target.name))
            .args([&source_ref, &preview.target.oid, &previous])
            .output(cancel)
            .await?;
        let record = MoveRecord {
            branch: preview.source.name.clone(),
            current: false,
            previous,
            after: preview.target.oid.clone(),
            index_before: None,
            stash: saved.clone(),
            fingerprint_after: String::new(),
        };
        return Ok((
            MoveOutcome::Done {
                head: record.after.clone(),
                fast_forward: true,
                saved,
            },
            Some(record),
        ));
    }

    let index_before = git
        .write(repo)
        .arg("write-tree")
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned();
    let cmd = if fast_forward {
        // Like git's rebase, but keeping local changes in other files, as
        // the preview allowed.
        git.write(repo).args(["merge", "--ff-only"])
    } else {
        // `-c`, not `--no-update-refs`…: older git ignores unknown settings
        // but refuses unknown options.
        git.write(repo).args([
            "-c",
            "rebase.updateRefs=false",
            "-c",
            "rebase.autoSquash=false",
            "-c",
            "rebase.rebaseMerges=false",
            "rebase",
            "--no-autostash",
        ])
    };
    let out = cmd
        .args(["--end-of-options", &preview.target.refname])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let status = working_status(git, repo, cancel).await?;
    stopped_or_failed(&out, &status, "rebase")?;
    if out.exit_code == Some(1) || !status.conflicted.is_empty() || status.in_progress.is_some() {
        return Ok((
            MoveOutcome::Conflicted {
                files: status.conflicted.into_iter().map(|c| c.path).collect(),
                saved,
            },
            None,
        ));
    }
    let head = ref_value(git, repo, &source_ref, cancel).await?;
    let record = MoveRecord {
        branch: preview.source.name.clone(),
        current: true,
        previous,
        after: head.clone(),
        index_before: Some(index_before),
        stash: saved.clone(),
        fingerprint_after: state_fingerprint(git, repo, cancel).await?,
    };
    Ok((
        MoveOutcome::Done {
            head,
            fast_forward,
            saved,
        },
        Some(record),
    ))
}
