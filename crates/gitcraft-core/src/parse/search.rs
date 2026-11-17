//! `git log -z` records for the history search (task 12.1): a commit id
//! and the text to look in, separated by U+001F.

use crate::parse::log::is_object_id;
use crate::{GitError, Result};

/// The whole message (subject and body).
pub const MESSAGE_FORMAT: &str = "--format=%H%x1f%B";
/// The author as git shows it: `Name <email>`.
pub const AUTHOR_FORMAT: &str = "--format=%H%x1f%an <%ae>";
/// Only the id (git already filtered, or the id is what is searched).
pub const OID_FORMAT: &str = "--format=%H";

/// One commit of the search stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRecord<'a> {
    pub oid: &'a str,
    /// What is searched; empty with [`OID_FORMAT`].
    pub text: &'a str,
}

/// Parses one NUL-terminated record (without the NUL).
pub fn parse_search_record(record: &str) -> Result<SearchRecord<'_>> {
    // git may print a newline between records.
    let record = record.trim_start_matches('\n');
    let (oid, text) = record.split_once('\u{1f}').unwrap_or((record, ""));
    if !is_object_id(oid) {
        let preview: String = record.chars().take(80).collect();
        return Err(GitError::Parse(format!(
            "unexpected search record: {preview:?}"
        )));
    }
    Ok(SearchRecord { oid, text })
}

/// For the search by file: each commit starts with U+001E and its id, and
/// is followed by the names of the matching files it changes (with `-c`, a
/// merge lists only those that differ from every parent).
pub const PATH_FORMAT: &str = "--format=%x1e%H";

/// Reads [`PATH_FORMAT`] output with `-c --name-only -z`, one NUL-terminated
/// token at a time, and keeps the commits that list at least one file: a
/// merge that only brings a change from one of its parents lists none.
#[derive(Debug, Default)]
pub struct PathRecords {
    pending: Option<String>,
    changes: bool,
}

impl PathRecords {
    /// Takes one token; returns the previous commit once it is known to
    /// change a matching file.
    pub fn push(&mut self, token: &str) -> Result<Option<String>> {
        // git may print a newline before a file list or a commit.
        let token = token.trim_start_matches('\n');
        if token.is_empty() {
            return Ok(None);
        }
        if let Some(oid) = token.strip_prefix('\u{1e}') {
            if !is_object_id(oid) {
                let preview: String = token.chars().take(80).collect();
                return Err(GitError::Parse(format!(
                    "unexpected search record: {preview:?}"
                )));
            }
            let done = self.finish();
            self.pending = Some(oid.to_owned());
            return Ok(done);
        }
        self.changes = true;
        Ok(None)
    }

    /// The last commit, if it changes a matching file.
    pub fn finish(&mut self) -> Option<String> {
        let changes = std::mem::take(&mut self.changes);
        self.pending.take().filter(|_| changes)
    }
}
