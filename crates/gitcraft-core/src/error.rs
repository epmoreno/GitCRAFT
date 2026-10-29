use std::io;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

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

    #[error("path does not exist or is not a directory: {}", .0.display())]
    PathNotFound(PathBuf),

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

    /// The file changed since its diff was shown; the request was not applied.
    #[error("the diff changed since it was shown")]
    StaleDiff,

    /// A discard cannot be undone because the file changed (or reappeared)
    /// since; nothing was modified.
    #[error("cannot undo the discard of {0}: it changed since")]
    UndoConflict(String),

    #[error("{0:?} is not a valid branch name")]
    InvalidBranchName(String),

    #[error("a branch named {0:?} already exists")]
    BranchExists(String),

    /// The branch moved or disappeared since it was shown; nothing was done.
    #[error("branch {0:?} changed since it was shown")]
    BranchChanged(String),

    /// The stash entry is no longer listed (dropped or popped elsewhere).
    #[error("stash entry {0} is no longer in the stash list")]
    StashChanged(String),

    #[error("there are no local changes to save")]
    NothingToStash,

    #[error("HEAD is detached: not on any branch")]
    DetachedHead,
}

/// What went wrong, in terms the UI can explain. Each kind has a translated
/// message (`errors.<kind>` in the i18n files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ErrorKind {
    GitNotFound,
    PathNotFound,
    NotARepository,
    BareRepository,
    DubiousOwnership,
    /// No user.name / user.email configured.
    IdentityUnknown,
    NothingToCommit,
    EmptyMessage,
    StaleDiff,
    UndoConflict,
    InvalidBranchName,
    BranchExists,
    BranchChanged,
    /// The branch is checked out in another worktree.
    BranchInWorktree,
    StashChanged,
    NothingToStash,
    /// The remote rejected the credentials, or none were available (no
    /// credential helper / SSH key): git cannot prompt.
    AuthFailed,
    DetachedHead,
    /// The remote could not be reached (network, host, path).
    RemoteUnreachable,
    Cancelled,
    TimedOut,
    Unknown,
}

impl GitError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::GitNotFound => ErrorKind::GitNotFound,
            Self::PathNotFound(_) => ErrorKind::PathNotFound,
            Self::Cancelled => ErrorKind::Cancelled,
            Self::TimedOut(_) => ErrorKind::TimedOut,
            Self::Failed {
                stderr,
                stdout_tail,
                ..
            } => classify_output(stderr, stdout_tail),
            Self::StaleDiff => ErrorKind::StaleDiff,
            Self::UndoConflict(_) => ErrorKind::UndoConflict,
            Self::InvalidBranchName(_) => ErrorKind::InvalidBranchName,
            Self::BranchExists(_) => ErrorKind::BranchExists,
            Self::BranchChanged(_) => ErrorKind::BranchChanged,
            Self::StashChanged(_) => ErrorKind::StashChanged,
            Self::NothingToStash => ErrorKind::NothingToStash,
            Self::DetachedHead => ErrorKind::DetachedHead,
            Self::Spawn(_) | Self::Io(_) | Self::Parse(_) => ErrorKind::Unknown,
        }
    }

    /// The original git output, for the "show details" part of error reports.
    pub fn details(&self) -> Option<String> {
        match self {
            Self::Failed {
                args,
                exit_code,
                stderr,
                stdout_tail,
            } => {
                let mut text = format!("$ git {}\n", args.join(" "));
                for part in [stdout_tail.trim_end(), stderr.trim_end()] {
                    if !part.is_empty() {
                        text.push_str(part);
                        text.push('\n');
                    }
                }
                if let Some(code) = exit_code {
                    text.push_str(&format!("(exit code {code})"));
                }
                Some(text.trim_end().to_owned())
            }
            Self::Spawn(_) | Self::Io(_) | Self::Parse(_) | Self::TimedOut(_) => {
                Some(self.to_string())
            }
            Self::UndoConflict(_) => Some(self.to_string()),
            Self::GitNotFound
            | Self::PathNotFound(_)
            | Self::Cancelled
            | Self::StaleDiff
            | Self::InvalidBranchName(_)
            | Self::BranchExists(_)
            | Self::BranchChanged(_)
            | Self::StashChanged(_)
            | Self::NothingToStash
            | Self::DetachedHead => None,
        }
    }
}

/// Maps git's stderr (always produced with `LC_ALL=C`) to an [`ErrorKind`].
pub fn classify_stderr(stderr: &str) -> ErrorKind {
    classify_output(stderr, "")
}

/// Like [`classify_stderr`], also looking at stdout: some commands (e.g.
/// `git commit` with nothing staged) explain themselves there.
pub fn classify_output(stderr: &str, stdout: &str) -> ErrorKind {
    // First match wins: authentication before the generic "could not read
    // from remote repository" that follows SSH failures.
    const PATTERNS: &[(&str, ErrorKind)] = &[
        ("terminal prompts disabled", ErrorKind::AuthFailed),
        ("could not read Username", ErrorKind::AuthFailed),
        ("could not read Password", ErrorKind::AuthFailed),
        ("Authentication failed", ErrorKind::AuthFailed),
        ("Invalid username or password", ErrorKind::AuthFailed),
        ("Permission denied (publickey", ErrorKind::AuthFailed),
        ("Host key verification failed", ErrorKind::AuthFailed),
        ("Could not resolve host", ErrorKind::RemoteUnreachable),
        ("Connection refused", ErrorKind::RemoteUnreachable),
        ("Connection timed out", ErrorKind::RemoteUnreachable),
        ("Operation timed out", ErrorKind::RemoteUnreachable),
        ("Network is unreachable", ErrorKind::RemoteUnreachable),
        (
            "does not appear to be a git repository",
            ErrorKind::RemoteUnreachable,
        ),
        (
            "Could not read from remote repository",
            ErrorKind::RemoteUnreachable,
        ),
        ("detected dubious ownership", ErrorKind::DubiousOwnership),
        ("not a git repository", ErrorKind::NotARepository),
        ("must be run in a work tree", ErrorKind::BareRepository),
        ("Please tell me who you are", ErrorKind::IdentityUnknown),
        ("Author identity unknown", ErrorKind::IdentityUnknown),
        ("Committer identity unknown", ErrorKind::IdentityUnknown),
        (
            "unable to auto-detect email address",
            ErrorKind::IdentityUnknown,
        ),
        ("empty commit message", ErrorKind::EmptyMessage),
        ("nothing to commit", ErrorKind::NothingToCommit),
        ("nothing added to commit", ErrorKind::NothingToCommit),
        ("no changes added to commit", ErrorKind::NothingToCommit),
        ("is not a valid branch name", ErrorKind::InvalidBranchName),
        ("No local changes to save", ErrorKind::NothingToStash),
        ("already exists", ErrorKind::BranchExists),
        ("is already checked out at", ErrorKind::BranchInWorktree),
        ("is already used by worktree", ErrorKind::BranchInWorktree),
        ("used by worktree at", ErrorKind::BranchInWorktree),
    ];
    PATTERNS
        .iter()
        .find(|(needle, _)| stderr.contains(needle) || stdout.contains(needle))
        .map_or(ErrorKind::Unknown, |&(_, kind)| kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(stderr: &str) -> GitError {
        GitError::Failed {
            args: vec!["rev-parse".into(), "--show-toplevel".into()],
            exit_code: Some(128),
            stderr: stderr.into(),
            stdout_tail: String::new(),
        }
    }

    #[test]
    fn classifies_known_stderr() {
        let cases = [
            (
                "fatal: not a git repository (or any of the parent directories): .git\n",
                ErrorKind::NotARepository,
            ),
            (
                "fatal: this operation must be run in a work tree\n",
                ErrorKind::BareRepository,
            ),
            (
                "fatal: detected dubious ownership in repository at '/Volumes/usb/repo'\n\
                 To add an exception for this directory, call:\n\n\
                 \tgit config --global --add safe.directory /Volumes/usb/repo\n",
                ErrorKind::DubiousOwnership,
            ),
            (
                "fatal: could not read Username for 'https://example.com': terminal prompts disabled\n",
                ErrorKind::AuthFailed,
            ),
            (
                "git@example.com: Permission denied (publickey).\nfatal: Could not read from remote repository.\n",
                ErrorKind::AuthFailed,
            ),
            (
                "fatal: unable to access 'https://nope.invalid/x.git/': Could not resolve host: nope.invalid\n",
                ErrorKind::RemoteUnreachable,
            ),
            (
                "fatal: '/tmp/missing.git' does not appear to be a git repository\nfatal: Could not read from remote repository.\n",
                ErrorKind::RemoteUnreachable,
            ),
            ("fatal: something new\n", ErrorKind::Unknown),
        ];
        for (stderr, kind) in cases {
            assert_eq!(failed(stderr).kind(), kind, "{stderr}");
        }
    }

    #[test]
    fn kinds_for_non_git_failures() {
        assert_eq!(GitError::GitNotFound.kind(), ErrorKind::GitNotFound);
        assert_eq!(
            GitError::PathNotFound("/nope".into()).kind(),
            ErrorKind::PathNotFound
        );
        assert_eq!(GitError::Cancelled.kind(), ErrorKind::Cancelled);
        assert_eq!(
            GitError::TimedOut(Duration::from_secs(1)).kind(),
            ErrorKind::TimedOut
        );
    }

    #[test]
    fn details_show_the_command_and_its_output() {
        let err = failed("fatal: not a git repository\n");
        assert_eq!(
            err.details().as_deref(),
            Some("$ git rev-parse --show-toplevel\nfatal: not a git repository\n(exit code 128)")
        );
        assert_eq!(GitError::Cancelled.details(), None);
    }

    #[test]
    fn serializes_kind_as_camel_case() {
        let json = serde_json::to_string(&ErrorKind::NotARepository).expect("serialize");
        assert_eq!(json, "\"notARepository\"");
    }
}
