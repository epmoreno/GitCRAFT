use std::path::Path;

use crate::parse::log::{Commit, LOG_FORMAT, parse_commit};
use crate::{CancellationToken, Git, Result};

/// The `git log` arguments for the commit graph: every branch, remote branch
/// and tag, children before parents. `include_head` adds a detached HEAD
/// (an attached HEAD is already covered by `--branches`).
pub fn graph_log_args(include_head: bool) -> Vec<&'static str> {
    let mut args = vec![
        "log",
        "--topo-order",
        "-z",
        LOG_FORMAT,
        "--branches",
        "--remotes",
        "--tags",
    ];
    if include_head {
        args.push("HEAD");
    }
    // Revisions end here even if a file is called e.g. "HEAD".
    args.push("--");
    args
}

/// Streams the graph's commits in topological order as git produces them.
///
/// Stops at the first malformed record and returns its parse error.
pub async fn stream_commits(
    git: &Git,
    repo: &Path,
    include_head: bool,
    cancel: &CancellationToken,
    mut on_commit: impl FnMut(Commit),
) -> Result<()> {
    let mut parse_error = None;
    let mut sink = |record: &[u8]| {
        if parse_error.is_some() || record.is_empty() {
            return;
        }
        match parse_commit(record) {
            Ok(commit) => on_commit(commit),
            Err(err) => parse_error = Some(err),
        }
    };
    git.read(repo)
        .args(graph_log_args(include_head))
        .stream(0, cancel, &mut sink, |_| {})
        .await?;
    parse_error.map_or(Ok(()), Err)
}
