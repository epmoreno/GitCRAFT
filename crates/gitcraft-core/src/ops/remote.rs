//! Talking to remotes: fetch, pull and push.
//!
//! Authentication is left to the user's credential helper or SSH agent;
//! GitCRAFT never stores or asks for credentials, and every command runs
//! with `GIT_TERMINAL_PROMPT=0`, so a missing credential fails fast with an
//! `authFailed` error instead of waiting for a prompt nobody can answer.
//! Progress comes from git's own `--progress` lines and every operation can
//! be cancelled.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ops::branch::{BlockingFile, LISTED_COMMITS, OrphanCommit};
use crate::ops::status::working_status;
use crate::parse::status::InProgress;
use crate::{CancellationToken, Git, GitError, Result};

/// Configured remotes (`git remote`).
pub async fn remotes(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<Vec<String>> {
    let out = git.read(repo).arg("remote").output(cancel).await?;
    Ok(crate::parse::refs::parse_remotes(&out.stdout_lossy()))
}

/// Remote names reach git's command line only if they are configured.
pub(crate) async fn known_remote(
    git: &Git,
    repo: &Path,
    remote: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    if remote.starts_with('-')
        || !remotes(git, repo, cancel)
            .await?
            .iter()
            .any(|r| r == remote)
    {
        return Err(GitError::Parse(format!("unknown remote {remote:?}")));
    }
    Ok(())
}

/// When `remote` was last fetched, as far as git can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LastFetch {
    /// Unix time in seconds; `None`: unknown (never fetched, or no trace).
    /// Exported as a 32-bit number, like the commit times.
    #[cfg_attr(feature = "specta", specta(type = Option<i32>))]
    pub at: Option<i64>,
    /// `at` is the last fetch of this remote (from `FETCH_HEAD`). Otherwise
    /// it is the last fetch that changed one of its remote branches (from
    /// their reflogs): it may have been fetched later, with nothing new.
    pub exact: bool,
}

/// A URL as `FETCH_HEAD` writes it: without credentials, a trailing "/"
/// or ".git".
fn fetch_head_url(url: &str) -> String {
    let mut url = url.trim().to_owned();
    if let Some(scheme) = url.find("://") {
        let rest = &url[scheme + 3..];
        let host_end = rest.find('/').unwrap_or(rest.len());
        if let Some(at) = rest[..host_end].rfind('@') {
            url = format!("{}{}", &url[..scheme + 3], &rest[at + 1..]);
        }
    }
    let mut url = url.trim_end_matches('/');
    if let Some(stripped) = url.strip_suffix(".git") {
        url = stripped.trim_end_matches('/');
    }
    url.to_owned()
}

/// When `remote` was last fetched: the time of `FETCH_HEAD` if the last
/// fetch included it, or else the newest fetch in its remote branches'
/// reflogs.
pub async fn last_fetch(
    git: &Git,
    repo: &Path,
    remote: &str,
    cancel: &CancellationToken,
) -> Result<LastFetch> {
    known_remote(git, repo, remote, cancel).await?;
    let urls = git
        .read(repo)
        .args(["remote", "get-url", "--all", "--end-of-options", remote])
        .output(cancel)
        .await?;
    let urls: BTreeSet<String> = urls.stdout_lossy().lines().map(fetch_head_url).collect();
    let path = git
        .read(repo)
        .args(["rev-parse", "--git-path", "FETCH_HEAD"])
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned();
    let path = std::path::PathBuf::from(crate::platform::native_path(&path));
    let path = if path.is_absolute() {
        path
    } else {
        repo.join(path)
    };
    if let Ok(text) = std::fs::read_to_string(&path) {
        let fetched = text
            .lines()
            .filter_map(|line| line.rsplit_once(" of "))
            .any(|(_, url)| urls.contains(&fetch_head_url(url)));
        let modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_secs()).ok());
        if fetched && modified.is_some() {
            return Ok(LastFetch {
                at: modified,
                exact: true,
            });
        }
    }
    // The reflog entries of its remote branches, written by fetch or pull.
    let refs = git
        .read(repo)
        .args(["for-each-ref", "--format=%(refname)"])
        .arg(format!("refs/remotes/{remote}/"))
        .output(cancel)
        .await?;
    let refs: Vec<String> = refs
        .stdout_lossy()
        .lines()
        .filter(|r| !r.ends_with("/HEAD"))
        .map(str::to_owned)
        .collect();
    let mut at = None;
    for refname in refs {
        let out = git
            .read(repo)
            .args(["log", "-g", "-z", "--date=unix", "--format=%gd%x1f%gs"])
            .args(["--end-of-options", &refname])
            .accept_exit_code(128)
            .output(cancel)
            .await?;
        let newest = String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter_map(|entry| {
                let (selector, subject) = entry.trim_start_matches('\n').split_once('\u{1f}')?;
                if !(subject.starts_with("fetch") || subject.starts_with("pull")) {
                    return None;
                }
                let time = selector.rsplit_once("@{")?.1.strip_suffix('}')?;
                time.parse::<i64>().ok()
            })
            .max();
        at = at.max(newest);
    }
    Ok(LastFetch { at, exact: false })
}

/// Downloads new commits and refs from `remote` (all remotes when `None`),
/// removing remote branches deleted there when `prune`. Never touches local
/// branches or the working tree.
pub async fn fetch(
    git: &Git,
    repo: &Path,
    remote: Option<&str>,
    prune: bool,
    cancel: &CancellationToken,
    on_progress: impl FnMut(&str),
) -> Result<()> {
    let mut cmd = git
        .write(repo)
        .args(["fetch", "--progress"])
        // Report progress from the start instead of after git's 2 s delay.
        .env("GIT_PROGRESS_DELAY", "0");
    if prune {
        cmd = cmd.arg("--prune");
    }
    match remote {
        Some(remote) => {
            known_remote(git, repo, remote, cancel).await?;
            cmd = cmd.arg(remote);
        }
        None => cmd = cmd.arg("--all"),
    }
    cmd.output_with_progress(cancel, on_progress).await?;
    Ok(())
}

/// How pull integrates the upstream's commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PullMode {
    /// A merge commit when both sides have commits (fast-forward otherwise).
    Merge,
    /// Local commits are replayed on top of the upstream.
    Rebase,
}

/// The branch HEAD is on; detached HEAD is an error (nothing to pull into
/// or push from).
pub async fn current_branch(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<String> {
    let out = git
        .read(repo)
        .args(["symbolic-ref", "--short", "-q", "HEAD"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let name = out.stdout_lossy().trim().to_owned();
    if out.exit_code != Some(0) || name.is_empty() {
        return Err(GitError::DetachedHead);
    }
    Ok(name)
}

/// Where a branch pulls from and pushes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    /// Remote name ("." for a local branch).
    pub remote: String,
    /// The branch on the remote, e.g. `refs/heads/main`.
    pub merge: String,
    /// The local ref that mirrors it, e.g. `refs/remotes/origin/main`.
    pub tracking: String,
}

impl Upstream {
    /// `origin/main`.
    pub fn short(&self) -> String {
        self.tracking
            .strip_prefix("refs/remotes/")
            .or_else(|| self.tracking.strip_prefix("refs/heads/"))
            .unwrap_or(&self.tracking)
            .to_owned()
    }
}

pub async fn upstream(
    git: &Git,
    repo: &Path,
    branch: &str,
    cancel: &CancellationToken,
) -> Result<Option<Upstream>> {
    let out = git
        .read(repo)
        .args([
            "for-each-ref",
            "--format=%(upstream)%1f%(upstream:remotename)%1f%(upstream:remoteref)",
        ])
        .arg(format!("refs/heads/{branch}"))
        .output(cancel)
        .await?;
    let line = out.stdout_lossy();
    let fields: Vec<&str> = line.trim_end().split('\u{1f}').collect();
    match fields[..] {
        [tracking, remote, merge] if !tracking.is_empty() && !remote.is_empty() => {
            Ok(Some(Upstream {
                remote: remote.to_owned(),
                merge: merge.to_owned(),
                tracking: tracking.to_owned(),
            }))
        }
        _ => Ok(None),
    }
}

async fn resolve(
    git: &Git,
    repo: &Path,
    rev: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify"])
        .arg(format!("{rev}^{{commit}}"))
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let oid = out.stdout_lossy().trim().to_owned();
    Ok((out.exit_code == Some(0) && !oid.is_empty()).then_some(oid))
}

/// Commits only in `a` and only in `b`.
async fn ahead_behind(
    git: &Git,
    repo: &Path,
    a: &str,
    b: &str,
    cancel: &CancellationToken,
) -> Result<(u32, u32)> {
    let out = git
        .read(repo)
        .args(["rev-list", "--left-right", "--count"])
        .arg(format!("{a}...{b}"))
        .output(cancel)
        .await?;
    let text = out.stdout_lossy();
    let mut counts = text.split_whitespace().map(str::parse::<u32>);
    match (counts.next(), counts.next()) {
        (Some(Ok(ahead)), Some(Ok(behind))) => Ok((ahead, behind)),
        _ => Err(GitError::Parse(format!("rev-list --count: {text:?}"))),
    }
}

/// The pull mode git itself would use: `branch.<name>.rebase`, then
/// `pull.rebase` ("true", "merges" or "interactive" mean rebase).
pub async fn configured_pull_mode(
    git: &Git,
    repo: &Path,
    branch: Option<&str>,
    cancel: &CancellationToken,
) -> Result<PullMode> {
    let mut keys = Vec::new();
    if let Some(branch) = branch {
        keys.push(format!("branch.{branch}.rebase"));
    }
    keys.push("pull.rebase".to_owned());
    for key in keys {
        let out = git
            .read(repo)
            .args(["config", "--get"])
            .arg(&key)
            .accept_exit_code(1)
            .output(cancel)
            .await?;
        let value = out.stdout_lossy().trim().to_ascii_lowercase();
        if out.exit_code == Some(0) && !value.is_empty() {
            let rebase = matches!(
                value.as_str(),
                "true" | "yes" | "on" | "1" | "merges" | "m" | "interactive" | "i"
            );
            return Ok(if rebase {
                PullMode::Rebase
            } else {
                PullMode::Merge
            });
        }
    }
    Ok(PullMode::Merge)
}

/// Why uncommitted changes stop a pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PullBlock {
    /// The incoming commits change these files, which have local changes.
    Overwrite,
    /// A rebase needs no uncommitted changes in tracked files at all.
    RebaseNeedsClean,
    /// A merge or rebase is already waiting for its conflicts.
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PullOutcome {
    /// Nothing new on the upstream.
    UpToDate { upstream: String },
    /// Brought `commits` commits in.
    Pulled {
        upstream: String,
        commits: u32,
        mode: PullMode,
        /// It was a fast-forward (no merge commit, nothing replayed).
        fast_forward: bool,
    },
    /// The branch has no upstream: nothing to pull from.
    NoUpstream { branch: String },
    /// The upstream's remote branch no longer exists.
    UpstreamGone { upstream: String },
    /// Uncommitted changes are in the way; nothing was touched.
    Blocked {
        files: Vec<BlockingFile>,
        reason: PullBlock,
    },
    /// These files changed on both sides and would conflict; nothing was
    /// touched. `exact`: git's merge said so (for a rebase, or with git
    /// older than 2.38, it is an estimate).
    WouldConflict {
        upstream: String,
        files: Vec<String>,
        exact: bool,
    },
    /// Started anyway at the user's request and stopped on conflicts in
    /// these files: a merge or rebase is waiting (see `InProgress`).
    Conflicted { files: Vec<String>, mode: PullMode },
}

fn names(stdout: &str) -> BTreeSet<String> {
    stdout
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Files a merge with `tracking` would bring in (changed since the merge base).
async fn incoming_files(
    git: &Git,
    repo: &Path,
    tracking: &str,
    cancel: &CancellationToken,
) -> Result<BTreeSet<String>> {
    let out = git
        .read(repo)
        .args(["diff", "--name-only", "-z", "--no-renames"])
        .arg(format!("HEAD...{tracking}"))
        .arg("--")
        .output(cancel)
        .await?;
    Ok(names(&out.stdout_lossy()))
}

async fn pull_blockers(
    git: &Git,
    repo: &Path,
    tracking: &str,
    mode: PullMode,
    cancel: &CancellationToken,
) -> Result<Option<(Vec<BlockingFile>, PullBlock)>> {
    let status = working_status(git, repo, cancel).await?;
    if status.in_progress.is_some() || !status.conflicted.is_empty() {
        let files = status
            .conflicted
            .iter()
            .map(|c| BlockingFile {
                path: c.path.clone(),
                untracked: false,
            })
            .collect();
        return Ok(Some((files, PullBlock::Unresolved)));
    }
    let incoming = incoming_files(git, repo, tracking, cancel).await?;
    let mut local = BTreeSet::new();
    for entry in status.staged.iter().chain(&status.unstaged) {
        local.insert(entry.path.clone());
        local.extend(entry.old_path.clone());
    }
    let untracked = status.untracked.iter().filter(|entry| {
        if entry.path.ends_with('/') {
            incoming.iter().any(|p| p.starts_with(entry.path.as_str()))
        } else {
            incoming.contains(&entry.path)
        }
    });
    let untracked: Vec<BlockingFile> = untracked
        .map(|e| BlockingFile {
            path: e.path.clone(),
            untracked: true,
        })
        .collect();

    let (tracked, reason) = match mode {
        PullMode::Rebase if !local.is_empty() => (local, PullBlock::RebaseNeedsClean),
        _ => (
            local.into_iter().filter(|p| incoming.contains(p)).collect(),
            PullBlock::Overwrite,
        ),
    };
    let mut files: Vec<BlockingFile> = tracked
        .into_iter()
        .map(|path| BlockingFile {
            path,
            untracked: false,
        })
        .collect();
    files.extend(untracked);
    Ok((!files.is_empty()).then_some((files, reason)))
}

/// Files that would conflict merging `tracking` into HEAD, from git's own
/// merge (`merge-tree --write-tree`, git ≥ 2.38) without touching anything;
/// with older git, the files changed on both sides. Returns whether the
/// answer is exact.
async fn predicted_conflicts(
    git: &Git,
    repo: &Path,
    tracking: &str,
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
            "HEAD",
            tracking,
        ])
        .accept_exit_code(1)
        .output(cancel)
        .await;
    if let Ok(out) = merge {
        let stdout = out.stdout_lossy();
        let files: BTreeSet<String> = stdout
            .split('\0')
            .skip(1)
            .take_while(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        return Ok((files.into_iter().collect(), true));
    }
    let theirs = incoming_files(git, repo, tracking, cancel).await?;
    let ours = git
        .read(repo)
        .args(["diff", "--name-only", "-z", "--no-renames"])
        .arg(format!("{tracking}...HEAD"))
        .arg("--")
        .output(cancel)
        .await?;
    let files = names(&ours.stdout_lossy())
        .into_iter()
        .filter(|p| theirs.contains(p))
        .collect();
    Ok((files, false))
}

/// Pulls the current branch's upstream with `mode`. Never forces: it first
/// fetches, then reports uncommitted changes in the way and predicted
/// conflicts without touching anything. With `allow_conflicts` it goes on
/// anyway and may stop with a merge/rebase waiting for the user.
pub async fn pull(
    git: &Git,
    repo: &Path,
    mode: PullMode,
    allow_conflicts: bool,
    cancel: &CancellationToken,
    on_progress: impl FnMut(&str),
) -> Result<PullOutcome> {
    let branch = current_branch(git, repo, cancel).await?;
    let Some(upstream) = upstream(git, repo, &branch, cancel).await? else {
        return Ok(PullOutcome::NoUpstream { branch });
    };
    let short = upstream.short();
    if upstream.remote != "." {
        fetch(
            git,
            repo,
            Some(&upstream.remote),
            false,
            cancel,
            on_progress,
        )
        .await?;
    }
    if resolve(git, repo, &upstream.tracking, cancel)
        .await?
        .is_none()
    {
        return Ok(PullOutcome::UpstreamGone { upstream: short });
    }
    let (ahead, behind) = ahead_behind(git, repo, "HEAD", &upstream.tracking, cancel).await?;
    if behind == 0 {
        return Ok(PullOutcome::UpToDate { upstream: short });
    }
    if let Some((files, reason)) =
        pull_blockers(git, repo, &upstream.tracking, mode, cancel).await?
    {
        return Ok(PullOutcome::Blocked { files, reason });
    }
    let fast_forward = ahead == 0;
    if !allow_conflicts && !fast_forward {
        let (files, exact) = predicted_conflicts(git, repo, &upstream.tracking, cancel).await?;
        if !files.is_empty() {
            return Ok(PullOutcome::WouldConflict {
                upstream: short,
                files,
                exact: exact && mode == PullMode::Merge,
            });
        }
    }

    let cmd = git
        .write(repo)
        // Never open an editor (merge messages are git's default).
        .env("GIT_EDITOR", "true");
    let cmd = match mode {
        PullMode::Merge => cmd.args(["merge", "--no-edit", "--no-stat"]),
        // Our own checks replace autostash: never stash behind the user's back.
        PullMode::Rebase => cmd.args(["rebase", "--no-autostash"]),
    };
    let out = cmd
        .arg(&upstream.tracking)
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let status = working_status(git, repo, cancel).await?;
    if !status.conflicted.is_empty() {
        return Ok(PullOutcome::Conflicted {
            files: status.conflicted.into_iter().map(|c| c.path).collect(),
            mode,
        });
    }
    if out.exit_code != Some(0) {
        return Err(GitError::Failed {
            args: vec![format!("{mode:?}").to_lowercase(), upstream.tracking],
            exit_code: out.exit_code,
            stdout_tail: out.stdout_lossy(),
            stderr: out.stderr,
        });
    }
    Ok(PullOutcome::Pulled {
        upstream: short,
        commits: behind,
        mode,
        fast_forward,
    })
}

/// "Save my changes, then pull": stashes every uncommitted change (as an
/// entry that is never dropped automatically) and pulls on the clean tree.
pub async fn stash_and_pull(
    git: &Git,
    repo: &Path,
    mode: PullMode,
    allow_conflicts: bool,
    cancel: &CancellationToken,
    on_progress: impl FnMut(&str),
) -> Result<PullOutcome> {
    crate::ops::stash::create_stash(git, repo, PULL_STASH_MESSAGE, true, cancel).await?;
    pull(git, repo, mode, allow_conflicts, cancel, on_progress).await
}

/// Message of the stash made by "save my changes, then pull".
pub const PULL_STASH_MESSAGE: &str = "GitCRAFT: antes de hacer pull";

/// Aborts the merge or rebase a pull left waiting on conflicts, going back
/// to the state before it (`git merge --abort` / `git rebase --abort`).
pub async fn abort_in_progress(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<()> {
    let args: &[&str] = match crate::ops::status::in_progress(git, repo, cancel).await? {
        Some(InProgress::Merge) => &["merge", "--abort"],
        Some(InProgress::Rebase) => &["rebase", "--abort"],
        Some(InProgress::CherryPick) => &["cherry-pick", "--abort"],
        Some(InProgress::Revert) => &["revert", "--abort"],
        None => return Ok(()),
    };
    git.write(repo).args(args).output(cancel).await?;
    Ok(())
}

/// Why the remote refused a push.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PushReject {
    /// The remote has commits this branch does not: pull first.
    RemoteAhead,
    /// `--force-with-lease`: the remote changed since it was checked.
    Stale,
    /// Refused by the remote itself (hooks, protected branch…).
    Remote,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PushOutcome {
    Pushed {
        upstream: String,
        /// Commits the remote did not have.
        commits: u32,
        /// The upstream was created by this push.
        set_upstream: bool,
        forced: bool,
    },
    /// The remote already has every commit of the branch.
    UpToDate { upstream: String },
    /// The branch has no upstream: offer to publish it to one of `remotes`.
    NoUpstream {
        branch: String,
        remotes: Vec<String>,
    },
    /// Refused; nothing changed on the remote. `detail` is git's summary.
    Rejected {
        upstream: String,
        reason: PushReject,
        detail: String,
    },
}

/// The rejection in `git push --porcelain` output, if any: lines are
/// `<flag>\t<from>:<to>\t<summary>`, flag `!` meaning rejected.
pub fn parse_push_rejection(stdout: &str) -> Option<(PushReject, String)> {
    stdout.lines().find_map(|line| {
        let mut fields = line.split('\t');
        let (Some("!"), Some(_), Some(summary)) = (fields.next(), fields.next(), fields.next())
        else {
            return None;
        };
        let reason = if summary.contains("stale info") {
            PushReject::Stale
        } else if summary.contains("fetch first") || summary.contains("non-fast-forward") {
            PushReject::RemoteAhead
        } else if summary.starts_with("[remote rejected]") {
            PushReject::Remote
        } else {
            PushReject::Other
        };
        Some((reason, summary.to_owned()))
    })
}

/// Pushes the current branch to its upstream, or publishes it to
/// `publish_to` (creating the upstream). `force_lease`: overwrite the
/// remote branch only if it still points to that commit
/// (`--force-with-lease`); never a plain `--force`.
pub async fn push(
    git: &Git,
    repo: &Path,
    publish_to: Option<&str>,
    force_lease: Option<&str>,
    cancel: &CancellationToken,
    on_progress: impl FnMut(&str),
) -> Result<PushOutcome> {
    let branch = current_branch(git, repo, cancel).await?;
    let local = format!("refs/heads/{branch}");
    let (remote, merge, tracking, set_upstream) = match publish_to {
        Some(remote) => {
            known_remote(git, repo, remote, cancel).await?;
            let tracking = format!("refs/remotes/{remote}/{branch}");
            (remote.to_owned(), local.clone(), tracking, true)
        }
        None => match upstream(git, repo, &branch, cancel).await? {
            Some(up) => (up.remote, up.merge, up.tracking, false),
            None => {
                return Ok(PushOutcome::NoUpstream {
                    branch,
                    remotes: remotes(git, repo, cancel).await?,
                });
            }
        },
    };
    let short = Upstream {
        remote: remote.clone(),
        merge: merge.clone(),
        tracking: tracking.clone(),
    }
    .short();

    // Commits the remote (as last fetched) does not have.
    let commits = match resolve(git, repo, &tracking, cancel).await? {
        Some(_) => ahead_behind(git, repo, "HEAD", &tracking, cancel).await?.0,
        None => {
            let out = git
                .read(repo)
                .args(["rev-list", "--count", "HEAD", "--not"])
                .arg(format!("--remotes={remote}"))
                .output(cancel)
                .await?;
            out.stdout_lossy().trim().parse().unwrap_or(0)
        }
    };
    if commits == 0 && force_lease.is_none() && !set_upstream {
        return Ok(PushOutcome::UpToDate { upstream: short });
    }

    let mut cmd = git
        .write(repo)
        .args(["push", "--porcelain", "--progress"])
        .env("GIT_PROGRESS_DELAY", "0");
    if set_upstream {
        cmd = cmd.arg("--set-upstream");
    }
    if let Some(expected) = force_lease {
        let expected = crate::ops::commit::checked_oid(expected)?;
        cmd = cmd.arg(format!("--force-with-lease={merge}:{expected}"));
    }
    // Both sides are full ref names: never mistaken for options.
    let out = cmd
        .arg(&remote)
        .arg(format!("{local}:{merge}"))
        .accept_exit_code(1)
        .output_with_progress(cancel, on_progress)
        .await?;
    let stdout = out.stdout_lossy();
    if let Some((reason, detail)) = parse_push_rejection(&stdout) {
        return Ok(PushOutcome::Rejected {
            upstream: short,
            reason,
            detail,
        });
    }
    if out.exit_code != Some(0) {
        return Err(GitError::Failed {
            args: vec!["push".into(), remote, format!("{local}:{merge}")],
            exit_code: out.exit_code,
            stdout_tail: stdout,
            stderr: out.stderr,
        });
    }
    Ok(PushOutcome::Pushed {
        upstream: short,
        commits,
        set_upstream,
        forced: force_lease.is_some(),
    })
}

/// What a forced push would overwrite on the remote, checked right after a
/// fetch: the commits only the remote branch has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ForcePushPreview {
    pub upstream: String,
    /// Where the remote branch is now: the lease (the push fails if it
    /// moves before it happens).
    pub expected: String,
    /// Remote commits that would be lost from the branch.
    pub overwritten: u32,
    /// The newest of them (at most [`LISTED_COMMITS`]).
    pub commits: Vec<OrphanCommit>,
    /// Local commits that would be pushed.
    pub ahead: u32,
}

/// Fetches the upstream's remote, then lists what forcing the push would
/// overwrite. Errors if there is no upstream or it no longer exists (a
/// normal push publishes it then).
pub async fn force_push_preview(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
    on_progress: impl FnMut(&str),
) -> Result<ForcePushPreview> {
    let branch = current_branch(git, repo, cancel).await?;
    let up = upstream(git, repo, &branch, cancel)
        .await?
        .ok_or_else(|| GitError::Parse(format!("{branch} has no upstream")))?;
    if up.remote != "." {
        fetch(git, repo, Some(&up.remote), false, cancel, on_progress).await?;
    }
    let expected = resolve(git, repo, &up.tracking, cancel)
        .await?
        .ok_or_else(|| GitError::Parse(format!("{} no longer exists", up.short())))?;
    let (ahead, overwritten) = ahead_behind(git, repo, "HEAD", &expected, cancel).await?;
    let listed = git
        .read(repo)
        .args([
            "log",
            "-z",
            "--format=%H%x1f%s%x1f%an%x1f%at",
            &format!("--max-count={LISTED_COMMITS}"),
            &expected,
            "--not",
            "HEAD",
        ])
        .output(cancel)
        .await?;
    let commits = listed
        .stdout_lossy()
        .split('\0')
        .filter(|r| !r.trim().is_empty())
        .map(|record| {
            let fields: Vec<&str> = record.split('\u{1f}').collect();
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
    Ok(ForcePushPreview {
        upstream: up.short(),
        expected,
        overwritten,
        commits,
        ahead,
    })
}

#[cfg(test)]
mod fetch_head_tests {
    use super::fetch_head_url;

    #[test]
    fn urls_are_compared_as_fetch_head_writes_them() {
        for (configured, written) in [
            ("https://github.com/a/b.git", "https://github.com/a/b"),
            (
                "https://user:secret@github.com/a/b.git/",
                "https://github.com/a/b",
            ),
            ("git@github.com:a/b.git", "git@github.com:a/b"),
            ("/srv/repos/b.git/", "/srv/repos/b"),
            ("/srv/repos/b", "/srv/repos/b"),
        ] {
            assert_eq!(fetch_head_url(configured), written, "{configured}");
        }
    }
}
