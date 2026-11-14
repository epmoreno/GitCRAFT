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

use serde::Serialize;

use crate::ops::branch::BlockingFile;
use crate::ops::in_progress::{CommitRef, commit_ref};
use crate::ops::reset::{remote_branches_containing, state_fingerprint};
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
    /// Merge commit: the message git would write (`git fmt-merge-msg`).
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
