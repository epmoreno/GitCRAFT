use gitcraft_core::{ErrorKind, GitError};
use serde::Serialize;

/// Error returned by every command. The UI shows a translated message for
/// `kind` and offers `details` (git's original output) on demand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub kind: ErrorKind,
    /// Technical summary (English), for logs.
    pub message: String,
    pub details: Option<String>,
}

impl From<GitError> for IpcError {
    fn from(err: GitError) -> Self {
        Self {
            kind: err.kind(),
            message: err.to_string(),
            details: err.details(),
        }
    }
}
