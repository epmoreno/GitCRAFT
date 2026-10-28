//! Reading a single commit: metadata, changed files and diffs.

use std::path::Path;

use crate::parse::commit::{CommitDetails, DETAILS_FORMAT, parse_commit_details};
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
