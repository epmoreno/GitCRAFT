//! Opt-in repository maintenance. Nothing here runs without the user asking.

use std::path::{Path, PathBuf};

use crate::{CancellationToken, Git, Result};

/// Above this many commits, a missing commit-graph noticeably delays the
/// first rows of the history (see docs/benchmarks.md).
pub const COMMIT_GRAPH_SUGGESTION_THRESHOLD: usize = 10_000;

/// Whether the repository has a commit-graph file (single or split chain).
pub async fn has_commit_graph(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<bool> {
    let out = git
        .read(repo)
        .args([
            "rev-parse",
            "--git-path",
            "objects/info/commit-graph",
            "--git-path",
            "objects/info/commit-graphs/commit-graph-chain",
        ])
        .output(cancel)
        .await?;
    // --git-path may print paths relative to the working directory.
    Ok(out
        .stdout_lossy()
        .lines()
        .map(|line| resolve(repo, line))
        .any(|path| path.is_file()))
}

fn resolve(repo: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    }
}

/// Writes a commit-graph with changed-path Bloom filters for every reachable
/// commit, reporting git's progress lines.
pub async fn write_commit_graph(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
    on_progress: impl FnMut(&str),
) -> Result<()> {
    git.write(repo)
        .args([
            "commit-graph",
            "write",
            "--reachable",
            "--changed-paths",
            "--progress",
        ])
        // Report progress from the start instead of after git's 2 s delay.
        .env("GIT_PROGRESS_DELAY", "0")
        .output_with_progress(cancel, on_progress)
        .await?;
    Ok(())
}
