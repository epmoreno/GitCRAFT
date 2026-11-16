//! `git blame --incremental` (task 12.5): git sends each group of lines as
//! soon as it knows which commit they come from, in any order.
//!
//! ```text
//! <oid> <line in that commit> <line in the file> <count>
//! author Ana                      ┐ only the first time this commit
//! author-mail <ana@example.com>   │ appears in the output
//! author-time 1700000000          │
//! summary Fix the header          │
//! previous <oid> <path>           │ (or "boundary": nothing before)
//! filename <path>                 ┘ always, ends the group
//! ```

use std::collections::HashSet;

use serde::Serialize;

use crate::parse::log::is_object_id;
use crate::{GitError, Result};

/// A commit some lines come from, sent once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BlameCommit {
    pub oid: String,
    pub author_name: String,
    pub author_email: String,
    /// Unix seconds.
    #[cfg_attr(feature = "specta", specta(type = i32))]
    pub author_time: i64,
    pub summary: String,
    /// The history starts here (a root commit, or a `--since` limit): there
    /// is nothing before it to blame.
    pub boundary: bool,
}

/// Consecutive lines of the file that come from one commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BlameChunk {
    pub oid: String,
    /// First line in the file, 1-based.
    pub line: u32,
    pub count: u32,
    /// First line in that commit's version of the file, 1-based.
    pub orig_line: u32,
    /// The file's path in that commit (it may have been renamed or moved
    /// from another file).
    pub path: String,
    /// The commit before it and the file's path there: "blame before this
    /// commit". `None` at a boundary.
    pub previous: Option<BlamePrevious>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BlamePrevious {
    pub oid: String,
    pub path: String,
}

/// What a line of the output completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlameEvent {
    /// The first time a commit appears: its details, then its first chunk.
    Commit(BlameCommit),
    Chunk(BlameChunk),
}

/// Reads the output one line at a time.
#[derive(Debug, Default)]
pub struct BlameParser {
    /// The group being read: its first line, then its fields.
    group: Option<Group>,
    /// Commits already sent.
    seen: HashSet<String>,
}

#[derive(Debug)]
struct Group {
    oid: String,
    orig_line: u32,
    line: u32,
    count: u32,
    commit: BlameCommit,
    previous: Option<BlamePrevious>,
}

impl BlameParser {
    /// Feeds one line (without its newline); returns what it completed.
    pub fn push(&mut self, line: &str) -> Result<Vec<BlameEvent>> {
        let Some(group) = self.group.as_mut() else {
            if line.is_empty() {
                return Ok(Vec::new());
            }
            self.group = Some(parse_group_start(line)?);
            return Ok(Vec::new());
        };
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "author" => group.commit.author_name = value.to_owned(),
            "author-mail" => {
                group.commit.author_email = value
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_owned();
            }
            "author-time" => group.commit.author_time = value.parse().unwrap_or(0),
            "summary" => group.commit.summary = value.to_owned(),
            "boundary" => group.commit.boundary = true,
            "previous" => {
                let (oid, path) = value.split_once(' ').unwrap_or((value, ""));
                group.previous = Some(BlamePrevious {
                    oid: oid.to_owned(),
                    path: unquote(path),
                });
            }
            "filename" => {
                let Some(group) = self.group.take() else {
                    return Ok(Vec::new());
                };
                let mut events = Vec::with_capacity(2);
                if self.seen.insert(group.oid.clone()) {
                    events.push(BlameEvent::Commit(BlameCommit {
                        oid: group.oid.clone(),
                        ..group.commit
                    }));
                }
                events.push(BlameEvent::Chunk(BlameChunk {
                    oid: group.oid,
                    line: group.line,
                    count: group.count,
                    orig_line: group.orig_line,
                    path: unquote(value),
                    previous: group.previous,
                }));
                return Ok(events);
            }
            // committer, author-tz…: not shown.
            _ => {}
        }
        Ok(Vec::new())
    }
}

fn parse_group_start(line: &str) -> Result<Group> {
    let bad = || GitError::Parse(format!("unexpected blame line: {line:?}"));
    let mut parts = line.split(' ');
    let oid = parts.next().filter(|o| is_object_id(o)).ok_or_else(bad)?;
    let mut number =
        || -> Result<u32> { parts.next().and_then(|n| n.parse().ok()).ok_or_else(bad) };
    let orig_line = number()?;
    let line_no = number()?;
    let count = number()?;
    Ok(Group {
        oid: oid.to_owned(),
        orig_line,
        line: line_no,
        count,
        commit: BlameCommit::default(),
        previous: None,
    })
}

/// git writes a path with unusual characters (quotes, backslashes,
/// control characters) between double quotes, C-style.
pub fn unquote(path: &str) -> String {
    let Some(inner) = path.strip_prefix('"').and_then(|p| p.strip_suffix('"')) else {
        return path.to_owned();
    };
    let mut bytes = Vec::with_capacity(inner.len());
    let mut chars = inner.bytes().peekable();
    while let Some(b) = chars.next() {
        if b != b'\\' {
            bytes.push(b);
            continue;
        }
        match chars.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(b'r') => bytes.push(b'\r'),
            Some(b'a') => bytes.push(0x07),
            Some(b'b') => bytes.push(0x08),
            Some(b'f') => bytes.push(0x0c),
            Some(b'v') => bytes.push(0x0b),
            Some(d @ b'0'..=b'7') => {
                // Three octal digits: one byte (of a UTF-8 sequence).
                let mut value = u32::from(d - b'0');
                for _ in 0..2 {
                    if let Some(&next @ b'0'..=b'7') = chars.peek() {
                        value = value * 8 + u32::from(next - b'0');
                        chars.next();
                    }
                }
                bytes.push(u8::try_from(value).unwrap_or(b'?'));
            }
            Some(other) => bytes.push(other),
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Parses a whole output (for tests and fixtures).
pub fn parse_blame(stdout: &str) -> Result<Vec<BlameEvent>> {
    let mut parser = BlameParser::default();
    let mut events = Vec::new();
    for line in stdout.lines() {
        events.extend(parser.push(line)?);
    }
    Ok(events)
}
