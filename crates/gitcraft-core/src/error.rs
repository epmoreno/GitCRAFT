use std::io;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use crate::redact::redact_credentials;

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

    #[error("{} already is a git repository", .0.display())]
    AlreadyARepository(PathBuf),

    #[error("{} already exists and is not an empty folder", .0.display())]
    DestinationNotEmpty(PathBuf),

    /// The repository has no commit yet: there is nothing to publish.
    #[error("the repository has no commits")]
    NoCommits,

    /// The repository already has remotes; publishing would add another.
    #[error("the repository already has a remote")]
    HasRemote,

    #[error("{0:?} is not a valid GitHub repository name")]
    InvalidRepoName(String),

    /// The GitHub account already has a repository with that name.
    #[error("{0} already exists on GitHub")]
    GhRepoExists(String),

    /// GitHub CLI (`gh`) is not installed (or not where GitCRAFT looks).
    #[error("gh executable not found")]
    GhNotFound,

    /// gh has no valid session for github.com.
    #[error("gh is not signed in to github.com")]
    GhNotLoggedIn,

    /// `gh` ran and failed.
    #[error("`gh {}` failed with exit code {exit_code:?}: {}", args.join(" "), stderr.trim())]
    GhFailed {
        args: Vec<String>,
        exit_code: Option<i32>,
        stderr: String,
        http: GhHttp,
    },
}

/// What GitHub answered to a failed `gh` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhHttp {
    /// Not a `gh api` call: only its exit code says anything.
    NotApi,
    /// `gh api` got no HTTP response at all.
    NoResponse,
    Status(u16),
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
    AlreadyARepository,
    DestinationNotEmpty,
    /// The remote could not be reached (network, host, path).
    RemoteUnreachable,
    /// The remote answered that the repository does not exist (or that the
    /// credentials in use cannot see it).
    RemoteNotFound,
    /// `index.lock` exists: another git process is running, or one crashed.
    IndexLocked,
    /// Local changes would be overwritten by the operation; git did nothing.
    LocalChangesOverwritten,
    /// The branches share no history; git refuses to merge them.
    UnrelatedHistories,
    /// The configuration only allows fast-forward and the branches diverged.
    NotFastForward,
    DiskFull,
    /// The system denied access to a file or folder of the repository.
    FilePermission,
    NoCommits,
    HasRemote,
    InvalidRepoName,
    /// The name is taken in the GitHub account; nothing was created.
    GhRepoExists,
    /// GitHub CLI (`gh`) is not installed.
    GhNotFound,
    /// `gh` has no valid session for github.com.
    GhNotLoggedIn,
    /// `gh` failed for another reason (its output says why).
    GhFailed,
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
            Self::AlreadyARepository(_) => ErrorKind::AlreadyARepository,
            Self::DestinationNotEmpty(_) => ErrorKind::DestinationNotEmpty,
            Self::NoCommits => ErrorKind::NoCommits,
            Self::HasRemote => ErrorKind::HasRemote,
            Self::InvalidRepoName(_) => ErrorKind::InvalidRepoName,
            Self::GhRepoExists(_) => ErrorKind::GhRepoExists,
            Self::GhNotFound => ErrorKind::GhNotFound,
            Self::GhNotLoggedIn => ErrorKind::GhNotLoggedIn,
            Self::GhFailed {
                exit_code, http, ..
            } => classify_gh(*exit_code, *http),
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
                Some(redact_credentials(text.trim_end()).into_owned())
            }
            Self::GhFailed {
                args,
                exit_code,
                stderr,
                ..
            } => {
                let mut text = format!("$ gh {}\n{}", args.join(" "), stderr.trim_end());
                if let Some(code) = exit_code {
                    text.push_str(&format!("\n(exit code {code})"));
                }
                Some(redact_credentials(text.trim_end()).into_owned())
            }
            Self::Spawn(_) | Self::Io(_) | Self::Parse(_) | Self::TimedOut(_) => {
                Some(redact_credentials(&self.to_string()).into_owned())
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
            | Self::DetachedHead
            | Self::AlreadyARepository(_)
            | Self::DestinationNotEmpty(_)
            | Self::NoCommits
            | Self::HasRemote
            | Self::InvalidRepoName(_)
            | Self::GhRepoExists(_)
            | Self::GhNotFound
            | Self::GhNotLoggedIn => None,
        }
    }
}

/// What a failed `gh` command means, from its documented exit codes (4:
/// authentication needed) and the HTTP status. Never from its wording.
fn classify_gh(exit_code: Option<i32>, http: GhHttp) -> ErrorKind {
    match (exit_code, http) {
        (Some(4), _) | (_, GhHttp::Status(401)) => ErrorKind::GhNotLoggedIn,
        (_, GhHttp::NoResponse) => ErrorKind::RemoteUnreachable,
        _ => ErrorKind::GhFailed,
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
        ("Permission denied, please try again", ErrorKind::AuthFailed),
        ("Repository not found", ErrorKind::RemoteNotFound),
        ("fatal: repository '", ErrorKind::RemoteNotFound),
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
        (
            "already exists and is not an empty directory",
            ErrorKind::DestinationNotEmpty,
        ),
        ("already exists", ErrorKind::BranchExists),
        ("index.lock': File exists", ErrorKind::IndexLocked),
        (
            "Another git process seems to be running",
            ErrorKind::IndexLocked,
        ),
        (
            "would be overwritten by",
            ErrorKind::LocalChangesOverwritten,
        ),
        (
            "refusing to merge unrelated histories",
            ErrorKind::UnrelatedHistories,
        ),
        ("Not possible to fast-forward", ErrorKind::NotFastForward),
        ("No space left on device", ErrorKind::DiskFull),
        ("insufficient permission", ErrorKind::FilePermission),
        ("Permission denied", ErrorKind::FilePermission),
        ("Operation not permitted", ErrorKind::FilePermission),
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
            (
                "ERROR: Repository not found.\nfatal: Could not read from remote repository.\n",
                ErrorKind::RemoteNotFound,
            ),
            (
                "remote: Repository not found.\nfatal: repository 'https://example.com/a/b.git/' not found\n",
                ErrorKind::RemoteNotFound,
            ),
            (
                "fatal: Unable to create '/r/.git/index.lock': File exists.\n\n\
                 Another git process seems to be running in this repository, e.g.\n",
                ErrorKind::IndexLocked,
            ),
            (
                "error: Your local changes to the following files would be overwritten by checkout:\n\ta.txt\n\
                 Please commit your changes or stash them before you switch branches.\nAborting\n",
                ErrorKind::LocalChangesOverwritten,
            ),
            (
                "fatal: refusing to merge unrelated histories\n",
                ErrorKind::UnrelatedHistories,
            ),
            (
                "fatal: Not possible to fast-forward, aborting.\n",
                ErrorKind::NotFastForward,
            ),
            (
                "error: unable to write file a.txt: No space left on device\n",
                ErrorKind::DiskFull,
            ),
            (
                "error: insufficient permission for adding an object to repository database .git/objects\n",
                ErrorKind::FilePermission,
            ),
            (
                "error: open(\"a.txt\"): Permission denied\n",
                ErrorKind::FilePermission,
            ),
            (
                "git@example.com: Permission denied, please try again.\n",
                ErrorKind::AuthFailed,
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
    fn details_never_show_credentials() {
        let err = GitError::Failed {
            args: vec!["fetch".into(), "https://ana:tok@example.com/r.git".into()],
            exit_code: Some(128),
            stderr: "fatal: unable to access 'https://ana:tok@example.com/r.git/'\n".into(),
            stdout_tail: String::new(),
        };
        let details = err.details().unwrap_or_default();
        assert!(!details.contains("tok"), "{details}");
        assert!(details.contains("https://***@example.com/r.git"));
    }

    #[test]
    fn serializes_kind_as_camel_case() {
        let json = serde_json::to_string(&ErrorKind::NotARepository).expect("serialize");
        assert_eq!(json, "\"notARepository\"");
    }
}
