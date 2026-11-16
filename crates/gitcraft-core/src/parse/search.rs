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
