//! `git diff-tree -z` file lists: `--name-status` and `--numstat`.

use serde::Serialize;

use crate::{GitError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    /// File type changed (e.g. file ↔ symlink).
    TypeChanged,
    Unmerged,
    /// Not tracked by git (working directory status only).
    Untracked,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FileChange {
    pub path: String,
    /// Previous path for renames and copies.
    pub old_path: Option<String>,
    pub status: FileStatus,
    /// Rename/copy similarity, 0–100.
    pub similarity: Option<u8>,
    /// Lines added/removed; `None` for binary files.
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

/// Parses `--name-status -z`: a status token, then one path (two for
/// renames and copies), each NUL-terminated.
pub fn parse_name_status(stdout: &[u8]) -> Result<Vec<FileChange>> {
    let mut fields = stdout
        .split(|&b| b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned());
    let mut changes = Vec::new();
    while let Some(token) = fields.next() {
        if token.is_empty() {
            continue; // trailing NUL
        }
        let (letter, score) = token.split_at(1);
        let status = match letter {
            "A" => FileStatus::Added,
            "M" => FileStatus::Modified,
            "D" => FileStatus::Deleted,
            "R" => FileStatus::Renamed,
            "C" => FileStatus::Copied,
            "T" => FileStatus::TypeChanged,
            "U" => FileStatus::Unmerged,
            _ => FileStatus::Unknown,
        };
        let mut next_path = || {
            fields
                .next()
                .filter(|p| !p.is_empty())
                .ok_or_else(|| GitError::Parse(format!("name-status: no path after {token:?}")))
        };
        let (old_path, path) = if matches!(status, FileStatus::Renamed | FileStatus::Copied) {
            let old = next_path()?;
            (Some(old), next_path()?)
        } else {
            (None, next_path()?)
        };
        changes.push(FileChange {
            path,
            old_path,
            status,
            similarity: score.parse().ok(),
            additions: None,
            deletions: None,
        });
    }
    Ok(changes)
}

/// One `--numstat -z` entry: counts (`None` for binary) and the new path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumStat {
    pub path: String,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

/// Parses `--numstat -z`: `added\tdeleted\tpath\0`, or for renames
/// `added\tdeleted\t\0old\0new\0`. Binary files show `-` counts.
pub fn parse_numstat(stdout: &[u8]) -> Result<Vec<NumStat>> {
    let mut fields = stdout
        .split(|&b| b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned());
    let mut stats = Vec::new();
    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        let mut parts = record.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(GitError::Parse(format!("numstat record: {record:?}")));
        };
        let path = if path.is_empty() {
            // Rename: the old and new paths follow as separate fields.
            let _old = fields.next();
            fields
                .next()
                .ok_or_else(|| GitError::Parse("numstat: rename without new path".into()))?
        } else {
            path.to_owned()
        };
        stats.push(NumStat {
            path,
            additions: added.parse().ok(),
            deletions: deleted.parse().ok(),
        });
    }
    Ok(stats)
}

/// Adds line counts to the name-status entries (matched by new path).
pub fn merge_counts(mut changes: Vec<FileChange>, stats: &[NumStat]) -> Vec<FileChange> {
    for change in &mut changes {
        if let Some(stat) = stats.iter().find(|s| s.path == change.path) {
            change.additions = stat.additions;
            change.deletions = stat.deletions;
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_status_with_renames() {
        let raw =
            b"M\0src/a.rs\0R087\0old name.txt\0new name.txt\0A\0\xc3\xb1.md\0D\0gone\0T\0link\0";
        let changes = parse_name_status(raw).expect("parse");
        let summary: Vec<_> = changes
            .iter()
            .map(|c| {
                (
                    c.status,
                    c.old_path.as_deref(),
                    c.path.as_str(),
                    c.similarity,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (FileStatus::Modified, None, "src/a.rs", None),
                (
                    FileStatus::Renamed,
                    Some("old name.txt"),
                    "new name.txt",
                    Some(87)
                ),
                (FileStatus::Added, None, "ñ.md", None),
                (FileStatus::Deleted, None, "gone", None),
                (FileStatus::TypeChanged, None, "link", None),
            ]
        );
    }

    #[test]
    fn parses_numstat_including_binary_and_renames() {
        let raw = b"3\t1\tsrc/a.rs\0-\t-\timg.png\0" as &[u8];
        let rename = b"0\t2\t\0old name.txt\0new name.txt\0" as &[u8];
        let stats = parse_numstat(&[raw, rename].concat()).expect("parse");
        assert_eq!(
            stats,
            [
                NumStat {
                    path: "src/a.rs".into(),
                    additions: Some(3),
                    deletions: Some(1)
                },
                NumStat {
                    path: "img.png".into(),
                    additions: None,
                    deletions: None
                },
                NumStat {
                    path: "new name.txt".into(),
                    additions: Some(0),
                    deletions: Some(2)
                },
            ]
        );
    }

    #[test]
    fn merges_counts_by_new_path() {
        let changes = parse_name_status(b"R100\0a\0b\0M\0c\0").expect("parse");
        let stats = parse_numstat(b"0\t0\t\0a\0b\x005\t2\tc\0").expect("parse");
        let merged = merge_counts(changes, &stats);
        assert_eq!(
            (merged[0].additions, merged[0].deletions),
            (Some(0), Some(0))
        );
        assert_eq!(
            (merged[1].additions, merged[1].deletions),
            (Some(5), Some(2))
        );
    }

    #[test]
    fn handles_empty_and_malformed_output() {
        assert!(parse_name_status(b"").expect("empty").is_empty());
        assert!(parse_numstat(b"").expect("empty").is_empty());
        assert!(parse_name_status(b"M\0").is_err(), "missing path");
        assert!(parse_numstat(b"garbage\0").is_err());
    }
}
