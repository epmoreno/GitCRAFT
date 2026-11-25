use gitcraft_core::redact::redact_credentials;
use gitcraft_core::{ErrorKind, ErrorSource, GitError};
use serde::Serialize;

/// Error returned by every command. The UI shows a translated message for
/// `kind` and offers `details` (the original output of git or gh, or GitCRAFT's
/// technical details: `source` says which) on demand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub kind: ErrorKind,
    /// Technical summary (English), for logs.
    pub message: String,
    pub details: Option<String>,
    pub source: ErrorSource,
}

impl From<GitError> for IpcError {
    fn from(err: GitError) -> Self {
        Self {
            kind: err.kind(),
            message: redact_credentials(&err.to_string()).into_owned(),
            details: err.details(),
            source: err.source_of_details(),
        }
    }
}
