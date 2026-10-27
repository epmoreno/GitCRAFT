use std::path::Path;

use serde::Serialize;

use crate::parse::refs::{
    GitRef, Head, REF_PATTERNS, REFS_FORMAT, parse_head, parse_refs, parse_remotes,
};
use crate::{CancellationToken, Git, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RepoRefs {
    pub head: Head,
    /// Local branches, remote branches and tags, sorted by full name.
    pub refs: Vec<GitRef>,
    /// Configured remotes, including those without fetched branches.
    pub remotes: Vec<String>,
}

impl RepoRefs {
    /// Whether the graph log must name HEAD explicitly (a detached HEAD is
    /// not reachable from any branch necessarily).
    pub fn head_is_detached(&self) -> bool {
        matches!(self.head, Head::Detached { .. })
    }
}

/// Reads branches, remote branches, tags, remotes and HEAD concurrently.
pub async fn read_refs(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<RepoRefs> {
    let refs = git
        .read(repo)
        .args(["for-each-ref", REFS_FORMAT])
        .args(REF_PATTERNS)
        .output(cancel);
    let remotes = git.read(repo).arg("remote").output(cancel);
    // Exit 1 means "detached" and "no commits yet" respectively.
    let symbolic = git
        .read(repo)
        .args(["symbolic-ref", "-q", "HEAD"])
        .accept_exit_code(1)
        .output(cancel);
    let head_oid = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "HEAD^{commit}"])
        .accept_exit_code(1)
        .output(cancel);
    let (refs, remotes, symbolic, head_oid) = tokio::try_join!(refs, remotes, symbolic, head_oid)?;

    let remotes = parse_remotes(&remotes.stdout_lossy());
    Ok(RepoRefs {
        head: parse_head(&symbolic.stdout_lossy(), &head_oid.stdout_lossy())?,
        refs: parse_refs(&refs.stdout_lossy(), &remotes)?,
        remotes,
    })
}
