//! Listing, creating, applying and deleting stash entries.
//!
//! Entries are identified by their commit id, not by `stash@{n}`, which
//! shifts whenever an entry is added or dropped. Applying never forces:
//! before running `git stash apply` we check which uncommitted changes it
//! would overwrite and whether it would conflict, and report that instead.
//! Nothing here drops an entry unless asked to (delete, or a pop that
//! applied cleanly).

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::ops::branch::BlockingFile;
use crate::ops::status::working_status;
use crate::parse::stash::{STASH_FORMAT, StashEntry, parse_stash_list};
use crate::{CancellationToken, Git, GitError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ApplyOutcome {
    /// Applied cleanly. `dropped`: it was a pop and the entry is gone.
    Applied { dropped: bool },
    /// Uncommitted changes (or untracked files) would be overwritten;
    /// nothing was touched.
    Blocked { files: Vec<BlockingFile> },
    /// These files changed on the current branch too and would conflict;
    /// nothing was touched. `exact`: git's merge said so (otherwise they
    /// only might, with git older than 2.40).
    WouldConflict { files: Vec<String>, exact: bool },
    /// Applied with conflict markers in these files (only when the user
    /// accepted that); the entry is kept, even for a pop.
    Conflicted { files: Vec<String> },
}

pub async fn list_stashes(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<Vec<StashEntry>> {
    let out = git
        .read(repo)
        .args(["stash", "list", "-z", STASH_FORMAT])
        .output(cancel)
        .await?;
    parse_stash_list(&out.stdout_lossy())
}

/// `stash@{n}` of the entry with commit `oid`, if it is still listed.
async fn selector(git: &Git, repo: &Path, oid: &str, cancel: &CancellationToken) -> Result<String> {
    let out = git
        .read(repo)
        .args(["stash", "list", "--format=%H"])
        .output(cancel)
        .await?;
    out.stdout_lossy()
        .lines()
        .position(|l| l.trim() == oid)
        .map(|i| format!("stash@{{{i}}}"))
        .ok_or_else(|| GitError::StashChanged(oid.to_owned()))
}

async fn top_of_stash(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<String> {
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "refs/stash"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    Ok(out.stdout_lossy().trim().to_owned())
}

/// Saves the uncommitted changes (and untracked files when asked) in a new
/// entry and returns its commit id. An empty `message` lets git name it.
pub async fn create_stash(
    git: &Git,
    repo: &Path,
    message: &str,
    include_untracked: bool,
    cancel: &CancellationToken,
) -> Result<String> {
    let before = top_of_stash(git, repo, cancel).await?;
    let mut cmd = git.write(repo).args(["stash", "push", "--quiet"]);
    if include_untracked {
        cmd = cmd.arg("--include-untracked");
    }
    let message = message.trim();
    if !message.is_empty() {
        cmd = cmd.arg("--message").arg(message);
    }
    cmd.output(cancel).await?;
    // With nothing to save git still exits 0 ("No local changes to save").
    let after = top_of_stash(git, repo, cancel).await?;
    if after.is_empty() || after == before {
        return Err(GitError::NothingToStash);
    }
    Ok(after)
}

fn names(stdout: &str) -> BTreeSet<String> {
    stdout
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Uncommitted changes that applying stash `oid` would overwrite.
async fn blocking_files(
    git: &Git,
    repo: &Path,
    oid: &str,
    untracked: bool,
    cancel: &CancellationToken,
) -> Result<Vec<BlockingFile>> {
    let worktree = git
        .read(repo)
        .args(["diff", "--name-only", "-z", "--no-renames"])
        .args([format!("{oid}^1"), oid.to_owned()])
        .arg("--")
        .output(cancel);
    let index = git
        .read(repo)
        .args(["diff", "--name-only", "-z", "--no-renames"])
        .args([format!("{oid}^1"), format!("{oid}^2")])
        .arg("--")
        .output(cancel);
    let status = working_status(git, repo, cancel);
    let (worktree, index, status) = tokio::try_join!(worktree, index, status)?;
    let mut touched = names(&worktree.stdout_lossy());
    touched.extend(names(&index.stdout_lossy()));

    let mut files = Vec::new();
    let mut local = BTreeSet::new();
    for entry in status.staged.iter().chain(&status.unstaged) {
        local.insert(entry.path.clone());
        local.extend(entry.old_path.clone());
    }
    local.extend(status.conflicted.iter().map(|c| c.path.clone()));
    for path in &local {
        if touched.contains(path) {
            files.push(BlockingFile {
                path: path.clone(),
                untracked: false,
            });
        }
    }
    for entry in &status.untracked {
        let hit = if entry.path.ends_with('/') {
            touched.iter().any(|t| t.starts_with(entry.path.as_str()))
        } else {
            touched.contains(&entry.path)
        };
        if hit {
            files.push(BlockingFile {
                path: entry.path.clone(),
                untracked: true,
            });
        }
    }
    // Untracked files saved in the entry cannot be restored over anything.
    if untracked {
        let saved = git
            .read(repo)
            .args(["ls-tree", "-r", "-z", "--name-only", "--full-tree"])
            .arg(format!("{oid}^3"))
            .output(cancel)
            .await?;
        for path in names(&saved.stdout_lossy()) {
            if std::fs::symlink_metadata(repo.join(&path)).is_ok()
                && !files.iter().any(|f| f.path == path)
            {
                files.push(BlockingFile {
                    path,
                    untracked: true,
                });
            }
        }
    }
    Ok(files)
}

/// Files that would conflict when applying `oid` onto HEAD. Uses git's own
/// merge (`merge-tree`, git ≥ 2.40) without touching anything; with older
/// git, the files changed both in the entry and on the branch since it was
/// made. Returns the files and whether the answer is exact.
async fn predicted_conflicts(
    git: &Git,
    repo: &Path,
    oid: &str,
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
        .arg(format!("--merge-base={oid}^1"))
        .args(["HEAD", oid])
        .accept_exit_code(1)
        .output(cancel)
        .await;
    if let Ok(out) = merge {
        let stdout = out.stdout_lossy();
        // "<tree>\0<file>\0<file>\0…"
        let files = stdout
            .split('\0')
            .skip(1)
            .take_while(|p| !p.is_empty())
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        return Ok((files.into_iter().collect(), true));
    }
    let entry = git
        .read(repo)
        .args(["diff", "--name-only", "-z", "--no-renames"])
        .args([format!("{oid}^1"), oid.to_owned()])
        .arg("--")
        .output(cancel);
    let branch = git
        .read(repo)
        .args(["diff", "--name-only", "-z", "--no-renames"])
        .args([format!("{oid}^1"), "HEAD".to_owned()])
        .arg("--")
        .output(cancel);
    let (entry, branch) = tokio::try_join!(entry, branch)?;
    let branch = names(&branch.stdout_lossy());
    let files = names(&entry.stdout_lossy())
        .into_iter()
        .filter(|p| branch.contains(p))
        .collect();
    Ok((files, false))
}

async fn has_untracked_part(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<bool> {
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify"])
        .arg(format!("{oid}^3"))
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    Ok(out.exit_code == Some(0))
}

/// Applies stash entry `oid` (and drops it when `pop` and it applied
/// cleanly). Never forces: reports blocking changes and predicted conflicts
/// without touching anything, unless `allow_conflicts` (then conflicts are
/// left marked in the files and the entry is kept).
pub async fn apply_stash(
    git: &Git,
    repo: &Path,
    oid: &str,
    pop: bool,
    allow_conflicts: bool,
    cancel: &CancellationToken,
) -> Result<ApplyOutcome> {
    selector(git, repo, oid, cancel).await?;
    let untracked = has_untracked_part(git, repo, oid, cancel).await?;
    let files = blocking_files(git, repo, oid, untracked, cancel).await?;
    if !files.is_empty() {
        return Ok(ApplyOutcome::Blocked { files });
    }
    if !allow_conflicts {
        let (files, exact) = predicted_conflicts(git, repo, oid, cancel).await?;
        if !files.is_empty() {
            return Ok(ApplyOutcome::WouldConflict { files, exact });
        }
    }
    run_apply(git, repo, oid, pop, cancel).await
}

/// "Save my changes, then apply": stashes the current uncommitted changes
/// (as a new entry, never dropped) and applies `oid` on the clean tree.
/// Conflicts with the branch are checked first, before anything is saved.
pub async fn stash_and_apply(
    git: &Git,
    repo: &Path,
    oid: &str,
    pop: bool,
    allow_conflicts: bool,
    cancel: &CancellationToken,
) -> Result<ApplyOutcome> {
    selector(git, repo, oid, cancel).await?;
    if !allow_conflicts {
        let (files, exact) = predicted_conflicts(git, repo, oid, cancel).await?;
        if !files.is_empty() {
            return Ok(ApplyOutcome::WouldConflict { files, exact });
        }
    }
    let label = format!("GitCRAFT: antes de aplicar {}", short(oid));
    create_stash(git, repo, &label, true, cancel).await?;
    apply_stash(git, repo, oid, pop, allow_conflicts, cancel).await
}

fn short(oid: &str) -> &str {
    oid.get(..7).unwrap_or(oid)
}

async fn run_apply(
    git: &Git,
    repo: &Path,
    oid: &str,
    pop: bool,
    cancel: &CancellationToken,
) -> Result<ApplyOutcome> {
    let target = selector(git, repo, oid, cancel).await?;
    let out = git
        .write(repo)
        .args(["stash", "apply", "--quiet", &target])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let status = working_status(git, repo, cancel).await?;
    if !status.conflicted.is_empty() {
        return Ok(ApplyOutcome::Conflicted {
            files: status.conflicted.into_iter().map(|c| c.path).collect(),
        });
    }
    if out.exit_code != Some(0) {
        return Err(GitError::Failed {
            args: vec!["stash".into(), "apply".into(), target],
            exit_code: out.exit_code,
            stdout_tail: out.stdout_lossy(),
            stderr: out.stderr,
        });
    }
    if pop {
        drop_stash(git, repo, oid, cancel).await?;
    }
    Ok(ApplyOutcome::Applied { dropped: pop })
}

/// Deletes entry `oid`. It stays recoverable for a while with
/// `git stash store -m <message> <oid>` (until git prunes it).
pub async fn drop_stash(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let target = selector(git, repo, oid, cancel).await?;
    git.write(repo)
        .args(["stash", "drop", "--quiet", &target])
        .output(cancel)
        .await?;
    Ok(())
}
