//! The history of a branch or of HEAD from git's reflog (task 11.5): where
//! it has been, to see a commit, create a branch there or reset to it.
//! Read-only.

use std::path::Path;

use serde::Serialize;

use crate::parse::reflog::{REFLOG_FORMAT, ReflogEntry, parse_reflog};
use crate::{CancellationToken, Git, GitError, Result};

/// Entries listed at most.
pub const MAX_ENTRIES: u32 = 200;

/// How long git keeps reflog entries before `git gc` expires them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Expiry {
    /// About this many days.
    Days { days: u32 },
    /// Never expired (`never`, `false`).
    Never,
}

/// The repository's `gc.reflogExpire` (entries whose commit a branch
/// still has) and `gc.reflogExpireUnreachable` (commits no branch has),
/// or git's defaults (90 and 30 days).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReflogExpiry {
    pub reachable: Expiry,
    pub unreachable: Expiry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Reflog {
    /// `HEAD` or a full branch name.
    pub refname: String,
    /// Newest first, at most [`MAX_ENTRIES`].
    pub entries: Vec<ReflogEntry>,
    /// There are older entries than these.
    pub more: bool,
    pub expiry: ReflogExpiry,
}

/// One setting as git reads it (`--type=expiry-date`: the cut-off time),
/// in days from now; `default` when it is not set.
async fn expiry(
    git: &Git,
    repo: &Path,
    key: &str,
    default: u32,
    cancel: &CancellationToken,
) -> Result<Expiry> {
    let out = git
        .read(repo)
        .args(["config", "--type=expiry-date", "--get", key])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    if out.exit_code != Some(0) {
        return Ok(Expiry::Days { days: default });
    }
    let cutoff: u64 = out.stdout_lossy().trim().parse().unwrap_or(0);
    if cutoff == 0 {
        return Ok(Expiry::Never);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = now.saturating_sub(cutoff).div_ceil(86_400);
    Ok(Expiry::Days {
        days: u32::try_from(days).unwrap_or(u32::MAX),
    })
}

/// The repository's reflog expiry settings.
pub async fn reflog_expiry(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<ReflogExpiry> {
    Ok(ReflogExpiry {
        reachable: expiry(git, repo, "gc.reflogExpire", 90, cancel).await?,
        unreachable: expiry(git, repo, "gc.reflogExpireUnreachable", 30, cancel).await?,
    })
}

/// Which of `oids` no branch, tag, remote branch or HEAD has.
async fn only_here(
    git: &Git,
    repo: &Path,
    oids: &[&str],
    cancel: &CancellationToken,
) -> Result<std::collections::BTreeSet<String>> {
    if oids.is_empty() {
        return Ok(Default::default());
    }
    let out = git
        .read(repo)
        .arg("rev-list")
        .args(oids)
        .args(["--not", "--branches", "--tags", "--remotes", "HEAD"])
        .output(cancel)
        .await?;
    let wanted: std::collections::BTreeSet<&str> = oids.iter().copied().collect();
    Ok(out
        .stdout_lossy()
        .lines()
        .filter(|oid| wanted.contains(oid))
        .map(str::to_owned)
        .collect())
}

/// The reflog of `refname`: `HEAD` or a local branch (`refs/heads/…`).
/// Empty when git keeps none (a branch deleted and created again starts a
/// new one; `core.logAllRefUpdates=false` keeps none).
pub async fn reflog(
    git: &Git,
    repo: &Path,
    refname: &str,
    cancel: &CancellationToken,
) -> Result<Reflog> {
    if refname != "HEAD" && !refname.starts_with("refs/heads/") {
        return Err(GitError::Parse(format!("not a branch: {refname:?}")));
    }
    let empty = Reflog {
        refname: refname.to_owned(),
        entries: Vec::new(),
        more: false,
        expiry: reflog_expiry(git, repo, cancel).await?,
    };
    let exists = git
        .read(repo)
        .args(["reflog", "exists", refname])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    if exists.exit_code != Some(0) {
        return Ok(empty);
    }
    let out = git
        .read(repo)
        .args(["log", "-g", "-z", "--date=unix", "--no-show-signature"])
        .arg(REFLOG_FORMAT)
        .arg(format!("--max-count={}", MAX_ENTRIES + 1))
        .args([refname, "--"])
        .output(cancel)
        .await?;
    let mut entries = parse_reflog(&out.stdout_lossy())?;
    let more = entries.len() > MAX_ENTRIES as usize;
    entries.truncate(MAX_ENTRIES as usize);
    let mut oids: Vec<&str> = entries.iter().map(|e| e.oid.as_str()).collect();
    oids.sort_unstable();
    oids.dedup();
    let lonely = only_here(git, repo, &oids, cancel).await?;
    for entry in &mut entries {
        entry.only_here = lonely.contains(&entry.oid);
    }
    Ok(Reflog {
        entries,
        more,
        ..empty
    })
}
