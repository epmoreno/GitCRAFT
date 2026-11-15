//! `git log -g` records: the reflog of a branch or of HEAD (task 11.5).

use serde::Serialize;

use crate::{GitError, Result};

/// Fields separated by U+001F, records by NUL (`-z`), with `--date=unix`:
/// commit, short commit, selector (`main@{<unix time>}`), what git noted
/// and the commit's subject.
pub const REFLOG_FORMAT: &str = "--format=%H%x1f%h%x1f%gd%x1f%gs%x1f%s";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReflogEntry {
    /// Position, newest first (`main@{index}`).
    pub index: u32,
    pub oid: String,
    pub short: String,
    /// When the ref moved there, in unix seconds.
    #[cfg_attr(feature = "specta", specta(type = i32))]
    pub time: i64,
    /// What git noted ("commit: …", "reset: moving to …"). Shown as it is,
    /// never interpreted.
    pub message: String,
    /// The subject of the commit it pointed to.
    pub subject: String,
    /// No branch, tag, remote branch or HEAD has the commit: only this
    /// history does, until git expires it.
    pub only_here: bool,
}

/// Parses `git log -g -z --date=unix` output in [`REFLOG_FORMAT`].
pub fn parse_reflog(stdout: &str) -> Result<Vec<ReflogEntry>> {
    stdout
        .split('\0')
        .map(|r| r.trim_start_matches('\n'))
        .filter(|r| !r.is_empty())
        .enumerate()
        .map(|(index, record)| {
            let fields: Vec<&str> = record.split('\u{1f}').collect();
            let [oid, short, selector, message, subject] = fields[..] else {
                return Err(GitError::Parse(format!("reflog record {record:?}")));
            };
            let time = selector
                .rsplit_once("@{")
                .and_then(|(_, rest)| rest.strip_suffix('}'))
                .and_then(|t| t.parse().ok())
                .ok_or_else(|| GitError::Parse(format!("reflog selector {selector:?}")))?;
            Ok(ReflogEntry {
                index: u32::try_from(index).unwrap_or(u32::MAX),
                oid: oid.to_owned(),
                short: short.to_owned(),
                time,
                message: message.to_owned(),
                subject: subject.to_owned(),
                only_here: false,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries_and_keeps_messages_as_they_are() {
        let out = [
            "a1\u{1f}a1s\u{1f}main@{1700000100}\u{1f}reset: moving to HEAD~1\u{1f}Base",
            "\na2\u{1f}a2s\u{1f}feature/x@{1700000000}\u{1f}commit: Añade: algo\u{1f}Añade: algo",
        ]
        .join("\0");
        let entries = parse_reflog(&out).expect("parse");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].index, 0);
        assert_eq!(entries[0].time, 1_700_000_100);
        assert_eq!(entries[0].message, "reset: moving to HEAD~1");
        assert_eq!(entries[1].index, 1);
        assert_eq!(entries[1].subject, "Añade: algo");
    }

    #[test]
    fn rejects_malformed_records() {
        assert!(parse_reflog("a\u{1f}b").is_err());
        assert!(parse_reflog("a\u{1f}b\u{1f}main@{x}\u{1f}m\u{1f}s").is_err());
        assert_eq!(parse_reflog("").expect("empty"), []);
    }
}
