//! Reading a single commit: metadata, changed files and diffs.

use std::path::Path;

use serde::Serialize;

use crate::parse::commit::{CommitDetails, DETAILS_FORMAT, parse_commit_details};
use crate::parse::diff::{FileDiff, parse_diff};
use crate::parse::diff_tree::{FileChange, merge_counts, parse_name_status, parse_numstat};
use crate::parse::log::is_object_id;
use crate::{CancellationToken, Git, GitError, Result};

/// Commit ids come from our own graph, but they still cross the IPC
/// boundary: only hex object ids reach git's command line.
pub(crate) fn checked_oid(oid: &str) -> Result<&str> {
    if is_object_id(oid) {
        Ok(oid)
    } else {
        Err(GitError::Parse(format!("not a commit id: {oid:?}")))
    }
}

pub async fn commit_details(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<CommitDetails> {
    let oid = checked_oid(oid)?;
    let out = git
        .read(repo)
        .args([
            "show",
            "-s",
            "--no-show-signature",
            "--no-notes",
            DETAILS_FORMAT,
            oid,
            "--",
        ])
        .output(cancel)
        .await?;
    parse_commit_details(&out.stdout)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CommitChanges {
    /// What the commit is compared with: its first parent, or `None` for a
    /// root commit (compared with the empty tree). Merges are shown against
    /// their first parent.
    pub base: Option<String>,
    pub files: Vec<FileChange>,
}

/// The first parent of `oid`, or `None` for a root commit.
pub async fn first_parent(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let oid = checked_oid(oid)?;
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", &format!("{oid}^1")])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let parent = out.stdout_lossy().trim().to_owned();
    Ok((!parent.is_empty()).then_some(parent))
}

/// `diff-tree` arguments comparing `oid` with `base` (or the empty tree).
pub(crate) fn diff_tree_range(oid: &str, base: Option<&str>) -> Vec<String> {
    match base {
        Some(parent) => vec![parent.to_owned(), oid.to_owned()],
        None => vec!["--root".to_owned(), oid.to_owned()],
    }
}

/// Files changed by `oid`, with line counts and rename detection.
pub async fn commit_changes(
    git: &Git,
    repo: &Path,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<CommitChanges> {
    let oid = checked_oid(oid)?;
    let base = first_parent(git, repo, oid, cancel).await?;
    let range = diff_tree_range(oid, base.as_deref());
    let list = |format: &'static str| {
        git.read(repo)
            .args(["diff-tree", "-r", "-z", "-M", "--no-commit-id", format])
            .args(&range)
            .arg("--")
            .output(cancel)
    };
    let (names, counts) = tokio::try_join!(list("--name-status"), list("--numstat"))?;
    let files = merge_counts(
        parse_name_status(&names.stdout)?,
        &parse_numstat(&counts.stdout)?,
    );
    Ok(CommitChanges { base, files })
}

/// Diffs longer than this are only sent when explicitly requested.
pub const DIFF_LINE_LIMIT: u32 = 5_000;
/// Output bigger than this is not even parsed unless requested.
const DIFF_BYTE_LIMIT: usize = 4 * 1024 * 1024;

/// The diff of one file in `oid` (against its first parent, or the empty
/// tree for a root commit). `old_path` is the previous name of a renamed or
/// copied file. Unless `full`, large diffs come back as `too_large` without
/// hunks.
pub async fn commit_file_diff(
    git: &Git,
    repo: &Path,
    oid: &str,
    path: &str,
    old_path: Option<&str>,
    full: bool,
    cancel: &CancellationToken,
) -> Result<FileDiff> {
    let oid = checked_oid(oid)?;
    if path.is_empty() {
        return Err(GitError::Parse("empty path".into()));
    }
    let base = first_parent(git, repo, oid, cancel).await?;
    let out = git
        .read(repo)
        // Paths come from the UI: never treat them as pathspec magic or globs.
        .env("GIT_LITERAL_PATHSPECS", "1")
        .args([
            "diff-tree",
            "-p",
            "-M",
            "-U3",
            "--no-ext-diff",
            "--no-commit-id",
        ])
        .args(diff_tree_range(oid, base.as_deref()))
        .arg("--")
        .args(old_path)
        .arg(path)
        .output(cancel)
        .await?;

    finish_diff(&out.stdout, full)
}

/// Parses diff output, leaving out the hunks of large diffs unless `full`.
/// The fingerprint always covers the whole output.
pub(crate) fn finish_diff(stdout: &[u8], full: bool) -> Result<FileDiff> {
    if !full && stdout.len() > DIFF_BYTE_LIMIT {
        let lines = stdout.iter().filter(|&&b| b == b'\n').count();
        return Ok(FileDiff {
            too_large: true,
            line_count: u32::try_from(lines).unwrap_or(u32::MAX),
            fingerprint: crate::parse::diff::fingerprint(stdout),
            ..FileDiff::empty()
        });
    }
    let mut diff = parse_diff(stdout)?;
    if !full && diff.line_count > DIFF_LINE_LIMIT {
        diff.hunks.clear();
        diff.too_large = true;
    }
    Ok(diff)
}
