//! Builds partial patches from a parsed diff and a selection of hunks or
//! lines, for `git apply` (stage, unstage, discard).
//!
//! The rule for lines that are *not* selected depends on the direction:
//!
//! - **Forward** (stage: apply the worktree→index diff to the index): an
//!   unselected `-` line must stay in the target, so it becomes context; an
//!   unselected `+` line must not appear, so it is dropped.
//! - **Reverse** (unstage / discard: the patch is applied with `-R` to the
//!   side that currently *has* the changes): an unselected `+` line stays, so
//!   it becomes context; an unselected `-` line is dropped.

use serde::{Deserialize, Serialize};

use crate::parse::diff::{FileDiff, LineKind};

/// Lines chosen in one hunk; `lines: None` means the whole hunk.
/// Indexes refer to [`crate::parse::diff::Hunk::lines`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HunkSelection {
    pub hunk: u32,
    pub lines: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Applied as is (`git apply [--cached]`).
    Forward,
    /// Applied with `git apply -R [--cached]`.
    Reverse,
}

/// The patch, or `None` when the selection contains no added or removed
/// line (nothing would change).
pub fn build_patch(
    diff: &FileDiff,
    selection: &[HunkSelection],
    direction: Direction,
) -> Option<String> {
    let mut out = String::new();
    for line in &diff.header {
        out.push_str(line);
        out.push('\n');
    }
    let mut changed = false;
    // Line-count difference (new − old) of the hunks emitted so far.
    let mut delta: i64 = 0;

    for (index, hunk) in diff.hunks.iter().enumerate() {
        let Some(chosen) = selection
            .iter()
            .find(|s| usize::try_from(s.hunk).ok() == Some(index))
        else {
            continue;
        };
        let is_selected = |i: usize| match &chosen.lines {
            None => true,
            Some(lines) => lines.iter().any(|&l| usize::try_from(l).ok() == Some(i)),
        };

        let mut body = String::new();
        let (mut old_count, mut new_count) = (0i64, 0i64);
        let mut hunk_changed = false;
        for (i, line) in hunk.lines.iter().enumerate() {
            let selected = is_selected(i);
            let prefix = match (line.kind, selected, direction) {
                (LineKind::Context, _, _) => ' ',
                (LineKind::Added, true, _) => '+',
                (LineKind::Removed, true, _) => '-',
                (LineKind::Removed, false, Direction::Forward)
                | (LineKind::Added, false, Direction::Reverse) => ' ',
                (LineKind::Added, false, Direction::Forward)
                | (LineKind::Removed, false, Direction::Reverse) => continue,
            };
            match prefix {
                '+' => new_count += 1,
                '-' => old_count += 1,
                _ => {
                    old_count += 1;
                    new_count += 1;
                }
            }
            hunk_changed |= prefix != ' ';
            body.push(prefix);
            body.push_str(&line.content);
            body.push('\n');
            if line.no_newline {
                body.push_str("\\ No newline at end of file\n");
            }
        }
        if !hunk_changed {
            continue;
        }
        changed = true;

        // The side the patch is matched against keeps its real position and
        // line count; the other side is shifted by the hunks emitted before.
        // In unified diffs an empty range points at the line *before* it,
        // so convert through the first line of the range.
        let first = |start: u32, count: u32| i64::from(start) + i64::from(count == 0);
        let start = |first: i64, count: i64| if count == 0 { first - 1 } else { first }.max(0);
        let (old_start, new_start) = match direction {
            Direction::Forward => {
                let old_first = first(hunk.old_start, hunk.old_lines);
                (
                    i64::from(hunk.old_start),
                    start(old_first + delta, new_count),
                )
            }
            Direction::Reverse => {
                let new_first = first(hunk.new_start, hunk.new_lines);
                (
                    start(new_first - delta, old_count),
                    i64::from(hunk.new_start),
                )
            }
        };
        out.push_str(&format!(
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"
        ));
        out.push_str(&body);
        delta += new_count - old_count;
    }
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::diff::parse_diff;

    const HEADER: &str =
        "diff --git a/f.txt b/f.txt\nindex 1111111..2222222 100644\n--- a/f.txt\n+++ b/f.txt\n";

    fn diff(body: &str) -> FileDiff {
        parse_diff(format!("{HEADER}{body}").as_bytes()).expect("parse")
    }

    fn all(hunk: u32) -> HunkSelection {
        HunkSelection { hunk, lines: None }
    }

    fn lines(hunk: u32, lines: &[u32]) -> HunkSelection {
        HunkSelection {
            hunk,
            lines: Some(lines.to_vec()),
        }
    }

    /// two → TWO, plus a new line after four.
    const ONE_HUNK: &str = "@@ -1,4 +1,5 @@\n one\n-two\n+TWO\n three\n four\n+five\n";

    #[test]
    fn whole_hunk_reproduces_the_hunk() {
        let patch = build_patch(&diff(ONE_HUNK), &[all(0)], Direction::Forward).expect("patch");
        assert_eq!(patch, format!("{HEADER}{ONE_HUNK}"));
    }

    #[test]
    fn forward_keeps_unselected_removals_as_context() {
        // Only "+five" (line 5): two stays as it is.
        let patch =
            build_patch(&diff(ONE_HUNK), &[lines(0, &[5])], Direction::Forward).expect("patch");
        assert_eq!(
            patch,
            format!("{HEADER}@@ -1,4 +1,5 @@\n one\n two\n three\n four\n+five\n")
        );
    }

    #[test]
    fn forward_drops_unselected_additions() {
        // Only "-two" (line 1): TWO and five are not staged.
        let patch =
            build_patch(&diff(ONE_HUNK), &[lines(0, &[1])], Direction::Forward).expect("patch");
        assert_eq!(
            patch,
            format!("{HEADER}@@ -1,4 +1,3 @@\n one\n-two\n three\n four\n")
        );
    }

    #[test]
    fn reverse_keeps_unselected_additions_as_context() {
        // Unstage only "+TWO"/"-two" (lines 1, 2); five stays staged.
        let patch =
            build_patch(&diff(ONE_HUNK), &[lines(0, &[1, 2])], Direction::Reverse).expect("patch");
        assert_eq!(
            patch,
            format!("{HEADER}@@ -1,5 +1,5 @@\n one\n-two\n+TWO\n three\n four\n five\n")
        );
    }

    #[test]
    fn later_hunks_are_shifted_by_earlier_ones() {
        let body = "@@ -1,3 +1,4 @@\n a\n+new\n b\n c\n@@ -10,3 +11,3 @@\n x\n-y\n+Y\n z\n";
        let d = diff(body);
        // Only the second hunk: in the index it still starts at line 10.
        let patch = build_patch(&d, &[all(1)], Direction::Forward).expect("patch");
        assert!(patch.contains("@@ -10,3 +10,3 @@\n"), "{patch}");
        // Both hunks: the second is shifted by the line the first added.
        let patch = build_patch(&d, &[all(0), all(1)], Direction::Forward).expect("patch");
        assert!(patch.contains("@@ -10,3 +11,3 @@\n"), "{patch}");
    }

    #[test]
    fn keeps_missing_newline_markers() {
        let d = diff(
            "@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+B\n\\ No newline at end of file\n",
        );
        let patch = build_patch(&d, &[all(0)], Direction::Forward).expect("patch");
        assert!(
            patch.ends_with("-b\n\\ No newline at end of file\n+B\n\\ No newline at end of file\n")
        );
    }

    #[test]
    fn selections_without_changes_produce_nothing() {
        // Line 0 and 3 are context.
        assert_eq!(
            build_patch(&diff(ONE_HUNK), &[lines(0, &[0, 3])], Direction::Forward),
            None
        );
        assert_eq!(build_patch(&diff(ONE_HUNK), &[], Direction::Forward), None);
        assert_eq!(
            build_patch(&diff(ONE_HUNK), &[all(7)], Direction::Forward),
            None
        );
    }
}
