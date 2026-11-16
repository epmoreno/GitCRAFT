//! The history of one file (task 12.3): the commits that changed it, from a
//! starting commit back, following renames. Read-only.
//!
//! The entries arrive in batches as git finds them, so the first ones can
//! be shown long before the end. git filters by itself, so there is no
//! percentage: only how many so far.

use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use crate::ops::search::{BATCH_INTERVAL, BATCH_RESULTS};
use crate::parse::file_history::{FILE_HISTORY_FORMAT, FileHistoryEntry, FileHistoryParser};
use crate::parse::log::is_object_id;
use crate::{CancellationToken, Git, GitError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FileHistorySummary {
    /// Entries in all.
    pub count: u32,
}

/// Where the history starts: `HEAD` or a commit id.
fn valid_start(rev: &str) -> bool {
    rev == "HEAD" || is_object_id(rev)
}

/// Streams the commits that changed `path` (its path in `rev`), newest
/// first, following renames (`--follow`). Works for a file that no longer
/// exists in `rev` too: its deletion is the first entry. Cancelling stops
/// git and returns [`GitError::Cancelled`].
pub async fn file_history(
    git: &Git,
    repo: &Path,
    path: &str,
    rev: &str,
    cancel: &CancellationToken,
    mut on_batch: impl FnMut(Vec<FileHistoryEntry>),
) -> Result<FileHistorySummary> {
    if !valid_start(rev) {
        return Err(GitError::Parse(format!(
            "file history: not a commit id: {rev:?}"
        )));
    }
    if path.is_empty() {
        return Ok(FileHistorySummary { count: 0 });
    }
    // No commit yet (a new repository): nothing to show.
    let exists = git
        .read(repo)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("{rev}^{{commit}}"))
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    if exists.exit_code != Some(0) {
        return Ok(FileHistorySummary { count: 0 });
    }

    let mut parser = FileHistoryParser::default();
    let mut pending = Vec::new();
    let mut count = 0u32;
    let mut last = Instant::now();
    let mut parse_error = None;
    let sink = |record: &[u8]| {
        if parse_error.is_some() {
            return;
        }
        match parser.push(&String::from_utf8_lossy(record)) {
            Ok(Some(entry)) => {
                count += 1;
                pending.push(entry);
                if pending.len() >= BATCH_RESULTS || last.elapsed() >= BATCH_INTERVAL {
                    last = Instant::now();
                    on_batch(std::mem::take(&mut pending));
                }
            }
            Ok(None) => {}
            Err(err) => parse_error = Some(err),
        }
    };
    git.read(repo)
        .args([
            "log",
            "--follow",
            "-z",
            "--name-status",
            FILE_HISTORY_FORMAT,
            rev,
            "--",
            path,
        ])
        .stream(0, cancel, sink, |_| {})
        .await?;
    if let Some(err) = parse_error {
        return Err(err);
    }
    if !pending.is_empty() {
        on_batch(pending);
    }
    Ok(FileHistorySummary { count })
}
