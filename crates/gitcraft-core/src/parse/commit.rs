//! Full commit metadata from `git show -s --format=…` (NUL-separated fields).

use serde::Serialize;

use crate::parse::log::is_object_id;
use crate::{GitError, Result};

/// The raw message (`%B`) goes last: it may contain anything except NUL.
pub const DETAILS_FORMAT: &str =
    "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%cn%x00%ce%x00%ct%x00%B";

const FIELD_COUNT: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Signature {
    pub name: String,
    pub email: String,
    /// Unix seconds.
    #[cfg_attr(feature = "specta", specta(type = i32))]
    pub time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CommitDetails {
    pub oid: String,
    /// First parent first.
    pub parents: Vec<String>,
    pub author: Signature,
    pub committer: Signature,
    /// Subject line.
    pub summary: String,
    /// Everything after the subject and its blank line; may be empty.
    pub body: String,
}

pub fn parse_commit_details(stdout: &[u8]) -> Result<CommitDetails> {
    let text = String::from_utf8_lossy(stdout);
    let fields: Vec<&str> = text.splitn(FIELD_COUNT, '\0').collect();
    let [oid, parents, an, ae, at, cn, ce, ct, message] = fields[..] else {
        return Err(GitError::Parse(format!(
            "commit details: expected {FIELD_COUNT} fields in {:?}",
            text.chars().take(120).collect::<String>()
        )));
    };
    if !is_object_id(oid) {
        return Err(GitError::Parse(format!("commit details: bad id {oid:?}")));
    }
    let parents: Vec<String> = parents.split_whitespace().map(str::to_owned).collect();
    if !parents.iter().all(|p| is_object_id(p)) {
        return Err(GitError::Parse("commit details: bad parent id".into()));
    }
    let time = |value: &str| {
        value
            .parse::<i64>()
            .map_err(|_| GitError::Parse(format!("commit details: bad time {value:?}")))
    };
    let (summary, body) = split_message(message);
    Ok(CommitDetails {
        oid: oid.to_owned(),
        parents,
        author: Signature {
            name: an.to_owned(),
            email: ae.to_owned(),
            time: time(at)?,
        },
        committer: Signature {
            name: cn.to_owned(),
            email: ce.to_owned(),
            time: time(ct)?,
        },
        summary,
        body,
    })
}

/// Splits a raw message into subject and body, the way git does for `%s`
/// and `%b`: the subject is the first paragraph joined into one line.
fn split_message(message: &str) -> (String, String) {
    let message = message.trim_end();
    let mut lines = message.lines();
    let mut subject = Vec::new();
    for line in lines.by_ref() {
        if line.trim().is_empty() {
            if subject.is_empty() {
                continue; // leading blank lines
            }
            break;
        }
        subject.push(line.trim());
    }
    let body = lines.collect::<Vec<_>>().join("\n");
    (subject.join(" "), body.trim_matches('\n').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";

    fn raw(parents: &str, message: &str) -> Vec<u8> {
        [
            A, parents, "Ana", "ana@x.es", "100", "Bea", "bea@x.es", "200", message,
        ]
        .join("\0")
        .into_bytes()
    }

    #[test]
    fn parses_signatures_and_message() {
        let d = parse_commit_details(&raw(B, "Fix it\n\nBecause.\n\nSigned-off-by: Ana\n"))
            .expect("parse");
        assert_eq!(d.oid, A);
        assert_eq!(d.parents, [B]);
        assert_eq!(
            d.author,
            Signature {
                name: "Ana".into(),
                email: "ana@x.es".into(),
                time: 100
            }
        );
        assert_eq!(d.committer.name, "Bea");
        assert_eq!(d.committer.time, 200);
        assert_eq!(d.summary, "Fix it");
        assert_eq!(d.body, "Because.\n\nSigned-off-by: Ana");
    }

    #[test]
    fn handles_subject_only_empty_and_multiline_subjects() {
        let d = parse_commit_details(&raw("", "Only a subject\n")).expect("parse");
        assert_eq!(
            (d.summary.as_str(), d.body.as_str()),
            ("Only a subject", "")
        );
        assert!(d.parents.is_empty());

        let d = parse_commit_details(&raw(B, "")).expect("empty");
        assert_eq!((d.summary.as_str(), d.body.as_str()), ("", ""));

        // Like %s, a subject spanning lines is joined with spaces.
        let d = parse_commit_details(&raw(B, "first\nsecond\n\nbody")).expect("parse");
        assert_eq!(d.summary, "first second");
        assert_eq!(d.body, "body");
    }

    #[test]
    fn keeps_unicode_and_inner_blank_lines() {
        let d = parse_commit_details(&raw(B, "Añade ñ\n\nuno\n\n\ndos\n")).expect("parse");
        assert_eq!(d.summary, "Añade ñ");
        assert_eq!(d.body, "uno\n\n\ndos");
    }

    #[test]
    fn rejects_malformed_output() {
        assert!(parse_commit_details(b"nope").is_err());
        let bad_time = [A, "", "a", "e", "x", "c", "e", "1", "m"].join("\0");
        assert!(parse_commit_details(bad_time.as_bytes()).is_err());
    }
}
