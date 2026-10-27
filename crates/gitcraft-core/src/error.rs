use std::io;
use std::time::Duration;

pub type Result<T, E = GitError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git executable not found")]
    GitNotFound,

    #[error("failed to start git: {0}")]
    Spawn(#[source] io::Error),

    #[error("I/O error while talking to git: {0}")]
    Io(#[from] io::Error),

    #[error("git operation was cancelled")]
    Cancelled,

    #[error("git operation timed out after {0:?}")]
    TimedOut(Duration),

    /// git ran and exited with a status the caller did not accept.
    #[error("`git {}` failed with exit code {exit_code:?}: {}", args.join(" "), stderr.trim())]
    Failed {
        args: Vec<String>,
        exit_code: Option<i32>,
        stderr: String,
        /// Last few KiB of stdout: some git commands report problems there
        /// (e.g. merge conflicts).
        stdout_tail: String,
    },

    #[error("unexpected git output: {0}")]
    Parse(String),
}
