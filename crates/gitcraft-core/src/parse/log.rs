//! `git log -z --format=…` records.

use serde::Serialize;

use crate::{GitError, Result};

/// Fields separated by U+001F; with `-z` each record ends with NUL.
/// Only the subject is requested: the full message is loaded on demand.
pub const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%ct%x1f%s";

const FIELD_COUNT: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Commit {
    pub oid: String,
    /// First parent first. Empty for root commits.
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    /// Unix seconds. (Exported as a plain JS number; i64 values fit easily.)
    #[cfg_attr(feature = "specta", specta(type = i32))]
    pub author_time: i64,
    #[cfg_attr(feature = "specta", specta(type = i32))]
    pub commit_time: i64,
    /// First line of the message.
    pub summary: String,
}

impl Commit {
    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }
}

/// Parses one NUL-terminated record (without the NUL).
pub fn parse_commit(record: &[u8]) -> Result<Commit> {
    let text = String::from_utf8_lossy(record);
    // Tolerate the newline git may print between records without -z.
    let text = text.trim_start_matches('\n');
    let fields: Vec<&str> = text.splitn(FIELD_COUNT, '\u{1f}').collect();
    let [
        oid,
        parents,
        author_name,
        author_email,
        author_time,
        commit_time,
        summary,
    ] = fields[..]
    else {
        return Err(bad_record(text, "wrong number of fields"));
    };

    if !is_object_id(oid) {
        return Err(bad_record(text, "invalid commit id"));
    }
    let parents: Vec<String> = parents.split_whitespace().map(str::to_owned).collect();
    if !parents.iter().all(|p| is_object_id(p)) {
        return Err(bad_record(text, "invalid parent id"));
    }
    let parse_time = |value: &str| {
        value
            .parse::<i64>()
            .map_err(|_| bad_record(text, "invalid timestamp"))
    };

    Ok(Commit {
        oid: oid.to_owned(),
        parents,
        author_name: author_name.to_owned(),
        author_email: author_email.to_owned(),
        author_time: parse_time(author_time)?,
        commit_time: parse_time(commit_time)?,
        summary: summary.to_owned(),
    })
}

/// SHA-1 (40) or SHA-256 (64) hex object id.
pub(crate) fn is_object_id(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn bad_record(record: &str, reason: &str) -> GitError {
    let preview: String = record.chars().take(120).collect();
    GitError::Parse(format!("git log record ({reason}): {preview:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";
    const C: &str = "3333333333333333333333333333333333333333";

    fn record(fields: &[&str]) -> Vec<u8> {
        fields.join("\u{1f}").into_bytes()
    }

    #[test]
    fn parses_a_regular_commit() {
        let commit = parse_commit(&record(&[
            A,
            B,
            "Ana",
            "ana@example.com",
            "1700000060",
            "1700000120",
            "Fix the thing",
        ]))
        .expect("parse");
        assert_eq!(
            commit,
            Commit {
                oid: A.into(),
                parents: vec![B.into()],
                author_name: "Ana".into(),
                author_email: "ana@example.com".into(),
                author_time: 1_700_000_060,
                commit_time: 1_700_000_120,
                summary: "Fix the thing".into(),
            }
        );
        assert!(!commit.is_merge());
    }

    #[test]
    fn parses_root_and_octopus_commits() {
        let root = parse_commit(&record(&[A, "", "a", "a@b", "1", "1", "root"])).expect("root");
        assert!(root.parents.is_empty());

        let octopus = parse_commit(&record(&[
            A,
            &format!("{B} {C} {A}"),
            "a",
            "a@b",
            "1",
            "1",
            "octopus",
        ]))
        .expect("octopus");
        assert_eq!(octopus.parents, [B, C, A]);
        assert!(octopus.is_merge());
    }

    #[test]
    fn keeps_unicode_empty_subjects_and_separators_in_the_subject() {
        let c = parse_commit(&record(&[A, B, "José Ñúñez", "j@é.es", "1", "1", ""]))
            .expect("empty subject");
        assert_eq!(c.author_name, "José Ñúñez");
        assert_eq!(c.summary, "");

        // A stray U+001F can only end up in the last field.
        let c = parse_commit(&record(&[A, B, "a", "a@b", "1", "1", "x\u{1f}y"])).expect("sep");
        assert_eq!(c.summary, "x\u{1f}y");
    }

    #[test]
    fn accepts_sha256_ids_and_negative_times() {
        let long = "a".repeat(64);
        let c = parse_commit(&record(&[&long, "", "a", "a@b", "-5", "0", "old"])).expect("parse");
        assert_eq!(c.oid, long);
        assert_eq!(c.author_time, -5);
    }

    #[test]
    fn rejects_malformed_records() {
        let cases: [&[&str]; 4] = [
            &[A, B, "a", "a@b", "1", "1"],
            &["not-a-sha", B, "a", "a@b", "1", "1", "s"],
            &[A, "zzz", "a", "a@b", "1", "1", "s"],
            &[A, B, "a", "a@b", "yesterday", "1", "s"],
        ];
        for fields in cases {
            assert!(
                matches!(parse_commit(&record(fields)), Err(GitError::Parse(_))),
                "{fields:?}"
            );
        }
    }
}
