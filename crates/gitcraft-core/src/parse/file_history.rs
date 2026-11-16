//! `git log --follow -z --name-status` for the history of one file (task
//! 12.3). Each commit starts with U+001E and its fields, separated by
//! U+001F; then come its `--name-status` tokens, each NUL-terminated:
//!
//! ```text
//! \x1e<oid>\x1f<author>\x1f<email>\x1f<time>\x1f<subject>\0
//! \nM\0path\0
//! \x1e…\0
//! \nR083\0old path\0new path\0
//! ```

use serde::Serialize;

use crate::parse::diff_tree::FileStatus;
use crate::parse::log::is_object_id;
use crate::{GitError, Result};

/// The commit header; `%x1e` marks where each commit starts.
pub const FILE_HISTORY_FORMAT: &str = "--format=%x1e%H%x1f%an%x1f%ae%x1f%at%x1f%s";

/// One commit that changed the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FileHistoryEntry {
    pub oid: String,
    pub author_name: String,
    pub author_email: String,
    /// Unix seconds.
    #[cfg_attr(feature = "specta", specta(type = i32))]
    pub author_time: i64,
    pub summary: String,
    /// The file's path in this commit.
    pub path: String,
    /// Its path before, when this commit renamed (or copied) it.
    pub old_path: Option<String>,
    pub status: FileStatus,
}

/// Builds entries from the NUL-separated tokens of the stream, one token
/// at a time, so it works as the output arrives.
#[derive(Debug, Default)]
pub struct FileHistoryParser {
    header: Option<Header>,
    /// The status token waiting for its path(s).
    status: Option<(FileStatus, Vec<String>)>,
}

#[derive(Debug, Clone)]
struct Header {
    oid: String,
    author_name: String,
    author_email: String,
    author_time: i64,
    summary: String,
}

impl FileHistoryParser {
    /// Feeds one token (without its NUL); returns an entry once complete.
    pub fn push(&mut self, token: &str) -> Result<Option<FileHistoryEntry>> {
        // git separates a commit's header from its files with a newline.
        let token = token.strip_prefix('\n').unwrap_or(token);
        if let Some(header) = token.strip_prefix('\u{1e}') {
            self.status = None;
            self.header = Some(parse_header(header)?);
            return Ok(None);
        }
        if token.is_empty() {
            return Ok(None);
        }
        let Some((status, paths)) = self.status.as_mut() else {
            self.status = Some((status_of(token), Vec::new()));
            return Ok(None);
        };
        paths.push(token.to_owned());
        let needed = if matches!(status, FileStatus::Renamed | FileStatus::Copied) {
            2
        } else {
            1
        };
        if paths.len() < needed {
            return Ok(None);
        }
        let status = *status;
        let mut paths = std::mem::take(paths);
        self.status = None;
        let header = self
            .header
            .clone()
            .ok_or_else(|| GitError::Parse("file history: a file before any commit".into()))?;
        let path = paths.pop().unwrap_or_default();
        let old_path = paths.pop();
        Ok(Some(FileHistoryEntry {
            oid: header.oid,
            author_name: header.author_name,
            author_email: header.author_email,
            author_time: header.author_time,
            summary: header.summary,
            path,
            old_path,
            status,
        }))
    }
}

fn status_of(token: &str) -> FileStatus {
    match token.get(..1) {
        Some("A") => FileStatus::Added,
        Some("M") => FileStatus::Modified,
        Some("D") => FileStatus::Deleted,
        Some("R") => FileStatus::Renamed,
        Some("C") => FileStatus::Copied,
        Some("T") => FileStatus::TypeChanged,
        _ => FileStatus::Unknown,
    }
}

fn parse_header(text: &str) -> Result<Header> {
    let fields: Vec<&str> = text.splitn(5, '\u{1f}').collect();
    let [oid, author_name, author_email, time, summary] = fields[..] else {
        return Err(bad(text));
    };
    if !is_object_id(oid) {
        return Err(bad(text));
    }
    Ok(Header {
        oid: oid.to_owned(),
        author_name: author_name.to_owned(),
        author_email: author_email.to_owned(),
        author_time: time.parse().map_err(|_| bad(text))?,
        summary: summary.to_owned(),
    })
}

fn bad(text: &str) -> GitError {
    let preview: String = text.chars().take(80).collect();
    GitError::Parse(format!("unexpected file history header: {preview:?}"))
}

/// Parses a whole `-z` output (for tests and fixtures).
pub fn parse_file_history(stdout: &str) -> Result<Vec<FileHistoryEntry>> {
    let mut parser = FileHistoryParser::default();
    let mut entries = Vec::new();
    for token in stdout.split('\0') {
        if let Some(entry) = parser.push(token)? {
            entries.push(entry);
        }
    }
    Ok(entries)
}
