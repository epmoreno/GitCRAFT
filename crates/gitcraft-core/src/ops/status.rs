use std::path::Path;

use crate::parse::status::{InProgress, WorkingStatus, parse_status};
use crate::{CancellationToken, Git, Result};

/// Status of the working directory and index. Runs as a read (no optional
/// locks), so it never competes with writes or rewrites the index.
pub async fn working_status(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<WorkingStatus> {
    let out = git
        .read(repo)
        .args([
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=normal",
            "--find-renames",
        ])
        .output(cancel);
    let (out, in_progress) = tokio::try_join!(out, in_progress(git, repo, cancel))?;
    let mut status = parse_status(&out.stdout)?;
    status.in_progress = in_progress;
    Ok(status)
}

/// Markers git leaves in the git dir while an operation waits for the user.
const MARKERS: [(&str, InProgress); 5] = [
    ("rebase-merge", InProgress::Rebase),
    ("rebase-apply", InProgress::Rebase),
    ("MERGE_HEAD", InProgress::Merge),
    ("CHERRY_PICK_HEAD", InProgress::CherryPick),
    ("REVERT_HEAD", InProgress::Revert),
];

/// The merge, rebase, cherry-pick or revert waiting to be continued or
/// aborted, if any.
pub async fn in_progress(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<Option<InProgress>> {
    let mut cmd = git.read(repo).arg("rev-parse");
    for (marker, _) in MARKERS {
        cmd = cmd.args(["--git-path", marker]);
    }
    let out = cmd.output(cancel).await?;
    let paths = out.stdout_lossy();
    Ok(paths.lines().zip(MARKERS).find_map(|(path, (_, kind))| {
        let path = Path::new(path.trim());
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            repo.join(path)
        };
        path.exists().then_some(kind)
    }))
}
