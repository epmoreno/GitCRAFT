//! `git status --porcelain=v2 -z --branch`.

use serde::Serialize;

use crate::parse::diff_tree::FileStatus;
use crate::{GitError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StatusEntry {
    pub path: String,
    /// Previous path of a rename or copy.
    pub old_path: Option<String>,
    pub status: FileStatus,
}

/// How a path conflicts during a merge, rebase, cherry-pick…
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ConflictKind {
    BothModified,
    BothAdded,
    BothDeleted,
    AddedByUs,
    AddedByThem,
    DeletedByUs,
    DeletedByThem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ConflictEntry {
    pub path: String,
    pub kind: ConflictKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BranchStatus {
    /// `None` when HEAD is detached.
    pub name: Option<String>,
    /// `None` before the first commit.
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WorkingStatus {
    pub branch: BranchStatus,
    /// Changes in the index (what the next commit will contain).
    pub staged: Vec<StatusEntry>,
    /// Changes in tracked files not yet staged.
    pub unstaged: Vec<StatusEntry>,
    pub untracked: Vec<StatusEntry>,
    pub conflicted: Vec<ConflictEntry>,
}

impl WorkingStatus {
    pub fn is_clean(&self) -> bool {
        self.staged.is_empty()
            && self.unstaged.is_empty()
            && self.untracked.is_empty()
            && self.conflicted.is_empty()
    }
}

fn status_of(code: char) -> FileStatus {
    match code {
        'A' => FileStatus::Added,
        'M' => FileStatus::Modified,
        'D' => FileStatus::Deleted,
        'R' => FileStatus::Renamed,
        'C' => FileStatus::Copied,
        'T' => FileStatus::TypeChanged,
        _ => FileStatus::Unknown,
    }
}

fn conflict_of(xy: &str) -> Option<ConflictKind> {
    Some(match xy {
        "UU" => ConflictKind::BothModified,
        "AA" => ConflictKind::BothAdded,
        "DD" => ConflictKind::BothDeleted,
        "AU" => ConflictKind::AddedByUs,
        "UA" => ConflictKind::AddedByThem,
        "DU" => ConflictKind::DeletedByUs,
        "UD" => ConflictKind::DeletedByThem,
        _ => return None,
    })
}

pub fn parse_status(stdout: &[u8]) -> Result<WorkingStatus> {
    let bad = |record: &str| GitError::Parse(format!("status record: {record:?}"));
    let mut status = WorkingStatus::default();
    let mut fields = stdout
        .split(|&b| b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned());

    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        if let Some(header) = record.strip_prefix("# ") {
            parse_header(header, &mut status.branch);
            continue;
        }
        let (kind, rest) = record.split_at(1);
        let rest = rest.strip_prefix(' ').ok_or_else(|| bad(&record))?;
        match kind {
            "1" | "2" => {
                // 1 XY sub mH mI mW hH hI path
                // 2 XY sub mH mI mW hH hI Xscore path <NUL> origPath
                let count = if kind == "1" { 8 } else { 9 };
                let parts: Vec<&str> = rest.splitn(count, ' ').collect();
                let (Some(xy), Some(path)) = (parts.first(), parts.get(count - 1)) else {
                    return Err(bad(&record));
                };
                let orig = if kind == "2" {
                    Some(fields.next().ok_or_else(|| bad(&record))?)
                } else {
                    None
                };
                let mut codes = xy.chars();
                let (x, y) = (codes.next().unwrap_or('.'), codes.next().unwrap_or('.'));
                let entry = |code: char| StatusEntry {
                    path: (*path).to_owned(),
                    old_path: matches!(code, 'R' | 'C').then(|| orig.clone()).flatten(),
                    status: status_of(code),
                };
                if x != '.' {
                    status.staged.push(entry(x));
                }
                if y != '.' {
                    status.unstaged.push(entry(y));
                }
            }
            "u" => {
                // u XY sub m1 m2 m3 mW h1 h2 h3 path
                let parts: Vec<&str> = rest.splitn(10, ' ').collect();
                let (Some(xy), Some(path)) = (parts.first(), parts.get(9)) else {
                    return Err(bad(&record));
                };
                status.conflicted.push(ConflictEntry {
                    path: (*path).to_owned(),
                    kind: conflict_of(xy).ok_or_else(|| bad(&record))?,
                });
            }
            "?" => status.untracked.push(StatusEntry {
                path: rest.to_owned(),
                old_path: None,
                status: FileStatus::Untracked,
            }),
            "!" => {} // ignored files are not requested, but harmless
            _ => return Err(bad(&record)),
        }
    }
    Ok(status)
}

fn parse_header(header: &str, branch: &mut BranchStatus) {
    let (key, value) = header.split_once(' ').unwrap_or((header, ""));
    match key {
        "branch.oid" => branch.oid = (value != "(initial)").then(|| value.to_owned()),
        "branch.head" => branch.name = (value != "(detached)").then(|| value.to_owned()),
        "branch.upstream" => branch.upstream = Some(value.to_owned()),
        "branch.ab" => {
            for part in value.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    branch.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    branch.behind = n.parse().unwrap_or(0);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "1111111111111111111111111111111111111111";

    fn ordinary(xy: &str, path: &str) -> String {
        format!("1 {xy} N... 100644 100644 100644 {H} {H} {path}")
    }

    #[test]
    fn splits_index_and_worktree_changes() {
        let raw = [
            "# branch.oid ".to_owned() + H,
            "# branch.head main".into(),
            "# branch.upstream origin/main".into(),
            "# branch.ab +2 -1".into(),
            ordinary("M.", "staged.txt"),
            ordinary(".M", "dir/unstaged file.txt"),
            ordinary("MM", "both.txt"),
            ordinary("A.", "new.txt"),
            ordinary(".D", "gone.txt"),
            format!("2 R. N... 100644 100644 100644 {H} {H} R100 new name.txt"),
            "old name.txt".into(),
            "? untracked/".into(),
            format!("u UU N... 100644 100644 100644 100644 {H} {H} {H} conflict.txt"),
            String::new(),
        ]
        .join("\0");
        let s = parse_status(raw.as_bytes()).expect("parse");

        assert_eq!(s.branch.name.as_deref(), Some("main"));
        assert_eq!(s.branch.oid.as_deref(), Some(H));
        assert_eq!(s.branch.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.branch.ahead, s.branch.behind), (2, 1));

        let staged: Vec<_> = s
            .staged
            .iter()
            .map(|e| (e.path.as_str(), e.status))
            .collect();
        assert_eq!(
            staged,
            [
                ("staged.txt", FileStatus::Modified),
                ("both.txt", FileStatus::Modified),
                ("new.txt", FileStatus::Added),
                ("new name.txt", FileStatus::Renamed),
            ]
        );
        assert_eq!(s.staged[3].old_path.as_deref(), Some("old name.txt"));

        let unstaged: Vec<_> = s
            .unstaged
            .iter()
            .map(|e| (e.path.as_str(), e.status))
            .collect();
        assert_eq!(
            unstaged,
            [
                ("dir/unstaged file.txt", FileStatus::Modified),
                ("both.txt", FileStatus::Modified),
                ("gone.txt", FileStatus::Deleted),
            ]
        );
        assert_eq!(s.untracked[0].path, "untracked/");
        assert_eq!(s.untracked[0].status, FileStatus::Untracked);
        assert_eq!(s.conflicted[0].kind, ConflictKind::BothModified);
        assert!(!s.is_clean());
    }

    #[test]
    fn reads_initial_and_detached_states() {
        let s = parse_status(b"# branch.oid (initial)\0# branch.head main\0").expect("initial");
        assert_eq!(s.branch.oid, None);
        assert_eq!(s.branch.name.as_deref(), Some("main"));
        assert!(s.is_clean());

        let raw = format!("# branch.oid {H}\0# branch.head (detached)\0");
        let s = parse_status(raw.as_bytes()).expect("detached");
        assert_eq!(s.branch.name, None);
    }

    #[test]
    fn rejects_malformed_records() {
        assert!(parse_status(b"1 M.\0").is_err());
        assert!(parse_status(b"x what\0").is_err());
        let rename_without_orig = format!("2 R. N... 100644 100644 100644 {H} {H} R100 a");
        assert!(parse_status(rename_without_orig.as_bytes()).is_err());
    }
}
