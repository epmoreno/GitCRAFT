use std::path::Path;

use crate::parse::status::{WorkingStatus, parse_status};
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
        .output(cancel)
        .await?;
    parse_status(&out.stdout)
}
