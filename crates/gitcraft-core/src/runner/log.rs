//! Observes every git invocation, for the command history.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use super::Access;
use crate::error::GitError;

/// Receives an event when each git process starts and ends.
pub trait CommandLog: Send + Sync {
    fn record(&self, event: CommandEvent);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEvent {
    Started {
        id: u64,
        cwd: PathBuf,
        /// The exact command line, quoted for a shell and with credentials
        /// redacted.
        command: String,
        access: Access,
        started: SystemTime,
    },
    Finished {
        id: u64,
        duration: Duration,
        outcome: CommandOutcome,
    },
}

/// How a git process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum CommandOutcome {
    Succeeded,
    /// Exited with a status the caller did not accept (`code` is missing
    /// when a signal stopped it).
    Failed {
        code: Option<i32>,
    },
    Cancelled,
    TimedOut,
    /// It could not be started or talked to.
    Error,
}

impl CommandOutcome {
    pub(super) fn of<T>(result: &Result<T, GitError>) -> Self {
        match result {
            Ok(_) => Self::Succeeded,
            Err(GitError::Failed { exit_code, .. }) => Self::Failed { code: *exit_code },
            Err(GitError::Cancelled) => Self::Cancelled,
            Err(GitError::TimedOut(_)) => Self::TimedOut,
            Err(_) => Self::Error,
        }
    }
}

/// A shared [`CommandLog`], cheap to clone into every command.
#[derive(Clone)]
pub(super) struct LogHandle(pub(super) Arc<dyn CommandLog>);

impl fmt::Debug for LogHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LogHandle")
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A started command whose end has not been recorded yet. If the command's
/// future is dropped before it ends, it is recorded as cancelled.
pub(super) struct Pending {
    log: LogHandle,
    id: u64,
    start: Instant,
    done: bool,
}

impl Pending {
    pub(super) fn start(log: LogHandle, cwd: PathBuf, command: String, access: Access) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        log.0.record(CommandEvent::Started {
            id,
            cwd,
            command,
            access,
            started: SystemTime::now(),
        });
        Self {
            log,
            id,
            start: Instant::now(),
            done: false,
        }
    }

    pub(super) fn finish(mut self, outcome: CommandOutcome) {
        self.record(outcome);
    }

    fn record(&mut self, outcome: CommandOutcome) {
        self.done = true;
        self.log.0.record(CommandEvent::Finished {
            id: self.id,
            duration: self.start.elapsed(),
            outcome,
        });
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if !self.done {
            self.record(CommandOutcome::Cancelled);
        }
    }
}

/// `args` as one line a shell would run unchanged: arguments with special
/// characters are single-quoted.
pub(super) fn shell_line<'a>(args: impl IntoIterator<Item = &'a str>) -> String {
    args.into_iter()
        .map(|arg| {
            let plain = !arg.is_empty()
                && arg.chars().all(|c| {
                    c.is_ascii_alphanumeric()
                        || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '@' | '%' | '+' | ',')
                });
            if plain {
                arg.to_owned()
            } else {
                format!("'{}'", arg.replace('\'', r"'\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_only_what_a_shell_would_split_or_expand() {
        assert_eq!(
            shell_line([
                "git",
                "log",
                "--format=%H%x1f%s",
                "--",
                "a b.txt",
                "it's",
                "$HOME",
                ""
            ]),
            r#"git log --format=%H%x1f%s -- 'a b.txt' 'it'\''s' '$HOME' ''"#
        );
    }
}
