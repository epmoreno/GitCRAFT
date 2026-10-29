use std::path::Path;

use serde::Serialize;

use crate::parse::refs::{
    GitRef, Head, REF_PATTERNS, REFS_FORMAT, RefKind, parse_head, parse_refs, parse_remotes,
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
    let mut refs = parse_refs(&refs.stdout_lossy(), &remotes)?;
    count_ahead_behind(git, repo, &mut refs, cancel).await?;
    Ok(RepoRefs {
        head: parse_head(&symbolic.stdout_lossy(), &head_oid.stdout_lossy())?,
        refs,
        remotes,
    })
}

/// Branches with an upstream counted at most (one `rev-list` each).
pub const MAX_COUNTED_BRANCHES: usize = 64;

/// Fills in `ahead`/`behind` (and `upstream_gone`) of local branches from
/// their upstream, which `refs` itself lists when it exists.
async fn count_ahead_behind(
    git: &Git,
    repo: &Path,
    refs: &mut [GitRef],
    cancel: &CancellationToken,
) -> Result<()> {
    let mut pending = tokio::task::JoinSet::new();
    let mut counted = 0;
    for index in 0..refs.len() {
        let branch = &refs[index];
        let Some(upstream) = branch.upstream.as_deref() else {
            continue;
        };
        if branch.kind != RefKind::Local {
            continue;
        }
        let target = refs
            .iter()
            .find(|r| r.kind == RefKind::Remote && r.short_name == upstream)
            .or_else(|| {
                refs.iter()
                    .find(|r| r.kind == RefKind::Local && r.short_name == upstream)
            })
            .map(|r| r.target.clone());
        let Some(target) = target else {
            refs[index].upstream_gone = true;
            continue;
        };
        if target == refs[index].target {
            refs[index].ahead = Some(0);
            refs[index].behind = Some(0);
            continue;
        }
        counted += 1;
        if counted > MAX_COUNTED_BRANCHES {
            continue;
        }
        let cmd = git
            .read(repo)
            .args(["rev-list", "--left-right", "--count"])
            .arg(format!("{}...{target}", refs[index].target));
        let cancel = cancel.clone();
        pending.spawn(async move { (index, cmd.output(&cancel).await) });
    }
    while let Some(joined) = pending.join_next().await {
        let Ok((index, out)) = joined else { continue };
        let out = out?;
        let text = out.stdout_lossy();
        let mut counts = text.split_whitespace().map(str::parse::<u32>);
        if let (Some(Ok(ahead)), Some(Ok(behind))) = (counts.next(), counts.next()) {
            refs[index].ahead = Some(ahead);
            refs[index].behind = Some(behind);
        }
    }
    Ok(())
}
