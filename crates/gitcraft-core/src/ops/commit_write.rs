//! Creating and amending commits.

use std::path::Path;

use serde::Serialize;

use crate::ops::commit::commit_details;
use crate::{CancellationToken, Git, GitError, Result};

/// Joins a summary and an optional description into a commit message.
pub fn compose_message(summary: &str, description: &str) -> String {
    let summary = summary.trim();
    let description = description.trim_end();
    let description = description.trim_start_matches('\n');
    if description.trim().is_empty() {
        format!("{summary}\n")
    } else {
        format!("{summary}\n\n{description}\n")
    }
}

/// Creates a commit from the staged changes (or amends the last one) with
/// `message`, passed on stdin: never through a shell or a temporary file.
/// Hooks and the user's commit settings (signing, cleanup…) apply as in a
/// terminal. Returns the new commit's id.
pub async fn create_commit(
    git: &Git,
    repo: &Path,
    message: &str,
    amend: bool,
    cancel: &CancellationToken,
) -> Result<String> {
    if message.trim().is_empty() {
        return Err(GitError::Failed {
            args: vec!["commit".into()],
            exit_code: None,
            stderr: "Aborting commit due to empty commit message.".into(),
            stdout_tail: String::new(),
        });
    }
    let mut cmd = git
        .write(repo)
        .args(["commit", "--quiet", "-F", "-"])
        // -F never opens an editor; this makes sure no config can.
        .env("GIT_EDITOR", "true");
    if amend {
        cmd = cmd.arg("--amend");
    }
    cmd.stdin(message).output(cancel).await?;
    let head = git
        .read(repo)
        .args(["rev-parse", "HEAD"])
        .output(cancel)
        .await?;
    Ok(head.stdout_lossy().trim().to_owned())
}

/// What "amend" would rewrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AmendInfo {
    pub oid: String,
    pub summary: String,
    pub body: String,
    /// Remote-tracking branches that already contain the commit: amending
    /// it then requires a force push and affects whoever fetched it.
    pub published_in: Vec<String>,
}

/// The last commit, for amending; `None` before the first commit.
pub async fn amend_info(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<Option<AmendInfo>> {
    let head = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "HEAD^{commit}"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let oid = head.stdout_lossy().trim().to_owned();
    if oid.is_empty() {
        return Ok(None);
    }
    let details = commit_details(git, repo, &oid, cancel).await?;
    let remotes = git
        .read(repo)
        .args([
            "branch",
            "--remotes",
            "--contains",
            &oid,
            "--format=%(refname:short)%09%(symref)",
        ])
        .output(cancel)
        .await?;
    let published_in = remotes
        .stdout_lossy()
        .lines()
        // Symbolic refs such as origin/HEAD are aliases, not places the
        // commit was pushed to.
        .filter_map(|line| match line.split_once('\t') {
            Some((name, "")) if !name.is_empty() => Some(name.to_owned()),
            _ => None,
        })
        .collect();
    Ok(Some(AmendInfo {
        oid,
        summary: details.summary,
        body: details.body,
        published_in,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_messages() {
        assert_eq!(compose_message("  Fix it  ", ""), "Fix it\n");
        assert_eq!(compose_message("Fix it", "  \n "), "Fix it\n");
        assert_eq!(
            compose_message("Fix it", "\n\nWhy.\n\n- a\n- b\n\n"),
            "Fix it\n\nWhy.\n\n- a\n- b\n"
        );
        assert_eq!(
            compose_message("Fix", "    indented"),
            "Fix\n\n    indented\n"
        );
    }
}
