//! Unified diff (`git diff-tree -p`) of one file.

use serde::Serialize;

use crate::{GitError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DiffLine {
    pub kind: LineKind,
    /// The line without its +/-/space prefix and newline.
    pub content: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    /// Followed by "\ No newline at end of file".
    pub no_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// Text after the second `@@` (usually the enclosing function).
    pub section: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FileDiff {
    pub binary: bool,
    /// Not sent because it is too big; ask again with `full`.
    pub too_large: bool,
    /// Diff lines (added, removed and context) in the whole diff.
    pub line_count: u32,
    /// Header lines such as "new file mode 100644" or "similarity index 90%".
    pub notes: Vec<String>,
    pub hunks: Vec<Hunk>,
    /// Identifies this exact diff. Partial stage/unstage/discard requests
    /// carry it back, so a file that changed meanwhile is never patched
    /// with stale line positions.
    pub fingerprint: String,
    /// The raw header of the first file ("diff --git" … "+++"), reused
    /// verbatim when building partial patches.
    #[serde(skip)]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub header: Vec<String>,
}

impl FileDiff {
    pub fn empty() -> Self {
        Self {
            binary: false,
            too_large: false,
            line_count: 0,
            notes: Vec::new(),
            hunks: Vec::new(),
            fingerprint: String::new(),
            header: Vec::new(),
        }
    }
}

/// FNV-1a: a small, dependency-free fingerprint (not a security measure).
pub fn fingerprint(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Header lines worth showing to the user.
const NOTE_PREFIXES: [&str; 8] = [
    "new file mode",
    "deleted file mode",
    "old mode",
    "new mode",
    "similarity index",
    "rename from",
    "rename to",
    "copy from",
];

pub fn parse_diff(stdout: &[u8]) -> Result<FileDiff> {
    let text = String::from_utf8_lossy(stdout);
    let mut diff = FileDiff {
        fingerprint: fingerprint(stdout),
        ..FileDiff::empty()
    };
    let mut in_header = true;
    let mut sections = 0;
    let (mut old_no, mut new_no) = (0u32, 0u32);

    for line in text.split_terminator('\n') {
        if line.starts_with("diff --git ") {
            in_header = true;
            sections += 1;
            if sections == 1 {
                diff.header.push(line.to_owned());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("@@ ") {
            let hunk = parse_hunk_header(rest)?;
            old_no = hunk.old_start;
            new_no = hunk.new_start;
            diff.hunks.push(hunk);
            in_header = false;
            continue;
        }
        if in_header {
            if sections <= 1 {
                diff.header.push(line.to_owned());
            }
            if line.starts_with("Binary files ") || line == "GIT binary patch" {
                diff.binary = true;
            } else if NOTE_PREFIXES.iter().any(|p| line.starts_with(p)) {
                diff.notes.push(line.to_owned());
            }
            continue;
        }
        let Some(hunk) = diff.hunks.last_mut() else {
            continue;
        };
        if line.starts_with('\\') {
            if let Some(last) = hunk.lines.last_mut() {
                last.no_newline = true;
            }
            continue;
        }
        // The prefix is always one ASCII byte, so slicing at 1 is safe.
        let (kind, content) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Added, &line[1..]),
            Some(b'-') => (LineKind::Removed, &line[1..]),
            Some(b' ') => (LineKind::Context, &line[1..]),
            // Some tools strip the space of empty context lines.
            None => (LineKind::Context, ""),
            Some(_) => {
                return Err(GitError::Parse(format!("unexpected diff line: {line:?}")));
            }
        };
        let (old_line, new_line) = match kind {
            LineKind::Added => {
                new_no += 1;
                (None, Some(new_no - 1))
            }
            LineKind::Removed => {
                old_no += 1;
                (Some(old_no - 1), None)
            }
            LineKind::Context => {
                old_no += 1;
                new_no += 1;
                (Some(old_no - 1), Some(new_no - 1))
            }
        };
        hunk.lines.push(DiffLine {
            kind,
            content: content.to_owned(),
            old_line,
            new_line,
            no_newline: false,
        });
        diff.line_count += 1;
    }
    Ok(diff)
}

/// Parses `-a[,b] +c[,d] @@ section` (after the leading `@@ `).
fn parse_hunk_header(rest: &str) -> Result<Hunk> {
    let bad = || GitError::Parse(format!("hunk header: @@ {rest:?}"));
    let (ranges, section) = rest.split_once(" @@").ok_or_else(bad)?;
    let (old, new) = ranges.split_once(' ').ok_or_else(bad)?;
    let range = |r: Option<&str>| -> Result<(u32, u32)> {
        let r = r.ok_or_else(bad)?;
        let (start, count) = r.split_once(',').unwrap_or((r, "1"));
        Ok((
            start.parse().map_err(|_| bad())?,
            count.parse().map_err(|_| bad())?,
        ))
    };
    let (old_start, old_lines) = range(old.strip_prefix('-'))?;
    let (new_start, new_lines) = range(new.strip_prefix('+'))?;
    Ok(Hunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        section: section.trim().to_owned(),
        lines: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODIFIED: &str = "\
diff --git a/src/a.rs b/src/a.rs
index 1111111..2222222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,4 +1,4 @@ fn main() {
 one
-two
+TWO
 three
 four
@@ -10 +10,2 @@
 ten
+eleven
";

    #[test]
    fn numbers_lines_across_hunks() {
        let d = parse_diff(MODIFIED.as_bytes()).expect("parse");
        assert!(!d.binary);
        assert_eq!(d.hunks.len(), 2);
        assert_eq!(d.line_count, 7);

        let h = &d.hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (1, 4, 1, 4)
        );
        assert_eq!(h.section, "fn main() {");
        let lines: Vec<_> = h
            .lines
            .iter()
            .map(|l| (l.kind, l.content.as_str(), l.old_line, l.new_line))
            .collect();
        assert_eq!(
            lines,
            [
                (LineKind::Context, "one", Some(1), Some(1)),
                (LineKind::Removed, "two", Some(2), None),
                (LineKind::Added, "TWO", None, Some(2)),
                (LineKind::Context, "three", Some(3), Some(3)),
                (LineKind::Context, "four", Some(4), Some(4)),
            ]
        );

        let h = &d.hunks[1];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (10, 1, 10, 2)
        );
        assert_eq!(h.lines[1].new_line, Some(11));
    }

    #[test]
    fn marks_missing_final_newlines() {
        let raw =
            "@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n";
        let d = parse_diff(raw.as_bytes()).expect("parse");
        assert!(d.hunks[0].lines.iter().all(|l| l.no_newline));
    }

    #[test]
    fn detects_binary_files_and_keeps_header_notes() {
        let raw = "diff --git a/x.png b/x.png\nnew file mode 100644\nindex 0000000..1111111\nBinary files /dev/null and b/x.png differ\n";
        let d = parse_diff(raw.as_bytes()).expect("parse");
        assert!(d.binary);
        assert!(d.hunks.is_empty());
        assert_eq!(d.notes, ["new file mode 100644"]);
    }

    #[test]
    fn keeps_content_that_looks_like_a_header() {
        let raw = "@@ -1,2 +1,2 @@\n--- not a header\n+++ not either\n";
        let d = parse_diff(raw.as_bytes()).expect("parse");
        let kinds: Vec<_> = d.hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.content.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                (LineKind::Removed, "-- not a header"),
                (LineKind::Added, "++ not either")
            ]
        );
    }

    #[test]
    fn rejects_a_line_starting_with_a_multibyte_character() {
        assert!(parse_diff("@@ -1 +1 @@\nñ\n".as_bytes()).is_err());
    }

    #[test]
    fn handles_empty_and_malformed_input() {
        let d = parse_diff(b"").expect("empty");
        assert!(d.hunks.is_empty() && !d.binary);
        assert!(parse_diff(b"@@ nonsense\n").is_err());
        assert!(parse_diff(b"@@ -1 +1 @@\n?what\n").is_err());
    }
}
