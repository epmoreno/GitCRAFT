//! Conflict markers git leaves in a working tree file:
//!
//! ```text
//! <<<<<<< ours label
//! our lines
//! ||||||| base label        (only with merge.conflictStyle diff3 / zdiff3)
//! common ancestor lines
//! =======
//! their lines
//! >>>>>>> their label
//! ```
//!
//! The parser keeps every byte: joining the segments back
//! ([`render_segments`]) gives the original text exactly, whatever its line
//! endings. No I/O here.

use serde::Serialize;

/// Marker length git uses unless the `conflict-marker-size` attribute says
/// otherwise.
pub const DEFAULT_MARKER_SIZE: usize = 7;

/// One conflicting block. Texts include their line endings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ConflictBlock {
    pub ours: String,
    /// The common ancestor's version of the block. Comes from the markers
    /// (`base_marker` is then set) or is computed separately (see
    /// `ops::conflict`), in which case it is not part of the file.
    pub base: Option<String>,
    pub theirs: String,
    /// Raw marker lines, with their line endings, so the file can be
    /// rebuilt byte for byte.
    pub ours_marker: String,
    pub base_marker: Option<String>,
    pub separator: String,
    pub theirs_marker: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Segment {
    /// Text outside any conflict block.
    Common {
        text: String,
    },
    Conflict {
        block: ConflictBlock,
    },
}

/// The markers do not form complete blocks (typically: the file was edited
/// by hand halfway). `line` is 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MalformedMarkers {
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    Ours,
    Base,
    Separator,
    Theirs,
}

/// The line without its line ending (`\n` or `\r\n`).
fn content(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

/// Which marker `line` is, if any: exactly `size` marker characters, then
/// the end of the line or (except for the separator) a space and a label.
/// Like git, a longer run of the same character is not a marker.
fn marker(line: &str, size: usize) -> Option<Marker> {
    let text = content(line);
    let first = text.chars().next()?;
    let kind = match first {
        '<' => Marker::Ours,
        '|' => Marker::Base,
        '=' => Marker::Separator,
        '>' => Marker::Theirs,
        _ => return None,
    };
    let run = text.chars().take_while(|&c| c == first).count();
    if run != size {
        return None;
    }
    let rest = &text[size..];
    let ok = match kind {
        Marker::Separator => rest.is_empty(),
        _ => rest.is_empty() || rest.starts_with(' '),
    };
    ok.then_some(kind)
}

enum State {
    Outside,
    Ours,
    Base,
    Theirs,
}

/// Splits `text` into common text and conflict blocks. Marker lines outside
/// a block other than the opening one (e.g. a Markdown `=======` heading
/// underline) are ordinary text, as they are for git.
pub fn parse_conflicts(text: &str, marker_size: usize) -> Result<Vec<Segment>, MalformedMarkers> {
    let mut segments = Vec::new();
    let mut common = String::new();
    let mut block = ConflictBlock {
        ours: String::new(),
        base: None,
        theirs: String::new(),
        ours_marker: String::new(),
        base_marker: None,
        separator: String::new(),
        theirs_marker: String::new(),
    };
    let mut state = State::Outside;
    let mut opened_at = 0u32;

    for (index, line) in text.split_inclusive('\n').enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let found = marker(line, marker_size);
        state = match (state, found) {
            (State::Outside, Some(Marker::Ours)) => {
                if !common.is_empty() {
                    segments.push(Segment::Common {
                        text: std::mem::take(&mut common),
                    });
                }
                block.ours_marker = line.to_owned();
                opened_at = number;
                State::Ours
            }
            (State::Outside, _) => {
                common.push_str(line);
                State::Outside
            }
            (State::Ours, Some(Marker::Base)) => {
                block.base_marker = Some(line.to_owned());
                block.base = Some(String::new());
                State::Base
            }
            (State::Ours | State::Base, Some(Marker::Separator)) => {
                block.separator = line.to_owned();
                State::Theirs
            }
            (State::Theirs, Some(Marker::Theirs)) => {
                block.theirs_marker = line.to_owned();
                let done = std::mem::replace(
                    &mut block,
                    ConflictBlock {
                        ours: String::new(),
                        base: None,
                        theirs: String::new(),
                        ours_marker: String::new(),
                        base_marker: None,
                        separator: String::new(),
                        theirs_marker: String::new(),
                    },
                );
                segments.push(Segment::Conflict { block: done });
                State::Outside
            }
            // Any other marker inside a block: the structure is broken.
            (_, Some(_)) => return Err(MalformedMarkers { line: number }),
            (State::Ours, None) => {
                block.ours.push_str(line);
                State::Ours
            }
            (State::Base, None) => {
                if let Some(base) = block.base.as_mut() {
                    base.push_str(line);
                }
                State::Base
            }
            (State::Theirs, None) => {
                block.theirs.push_str(line);
                State::Theirs
            }
        };
    }

    if !matches!(state, State::Outside) {
        return Err(MalformedMarkers { line: opened_at });
    }
    if !common.is_empty() {
        segments.push(Segment::Common { text: common });
    }
    Ok(segments)
}

/// The file the segments came from, byte for byte. A base that was computed
/// (no `base_marker`) is not part of the file and is left out.
pub fn render_segments(segments: &[Segment]) -> String {
    let mut out = String::new();
    for segment in segments {
        match segment {
            Segment::Common { text } => out.push_str(text),
            Segment::Conflict { block } => {
                out.push_str(&block.ours_marker);
                out.push_str(&block.ours);
                if let Some(marker) = &block.base_marker {
                    out.push_str(marker);
                    out.push_str(block.base.as_deref().unwrap_or_default());
                }
                out.push_str(&block.separator);
                out.push_str(&block.theirs);
                out.push_str(&block.theirs_marker);
            }
        }
    }
    out
}

/// 1-based numbers and text (without line ending) of the lines of `text`
/// that are conflict markers, wherever they are.
pub fn marker_lines(text: &str, marker_size: usize) -> Vec<(u32, String)> {
    text.split_inclusive('\n')
        .enumerate()
        .filter(|(_, line)| marker(line, marker_size).is_some())
        .map(|(i, line)| {
            (
                u32::try_from(i + 1).unwrap_or(u32::MAX),
                content(line).to_owned(),
            )
        })
        .collect()
}

pub fn conflict_count(segments: &[Segment]) -> usize {
    segments
        .iter()
        .filter(|s| matches!(s, Segment::Conflict { .. }))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(segments: &[Segment]) -> Vec<&ConflictBlock> {
        segments
            .iter()
            .filter_map(|s| match s {
                Segment::Conflict { block } => Some(block),
                Segment::Common { .. } => None,
            })
            .collect()
    }

    #[test]
    fn plain_merge_style() {
        let text = "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\nz\n";
        let segments = parse_conflicts(text, 7).expect("parse");
        assert_eq!(segments.len(), 3);
        let b = blocks(&segments)[0];
        assert_eq!(b.ours, "ours\n");
        assert_eq!(b.theirs, "theirs\n");
        assert_eq!(b.base, None);
        assert_eq!(b.ours_marker, "<<<<<<< HEAD\n");
        assert_eq!(b.theirs_marker, ">>>>>>> feature\n");
        assert_eq!(render_segments(&segments), text);
    }

    #[test]
    fn diff3_base_and_empty_sides() {
        let text = "<<<<<<< HEAD\n||||||| base\nold\n=======\nnew\n>>>>>>> x\n";
        let segments = parse_conflicts(text, 7).expect("parse");
        let b = blocks(&segments)[0];
        assert_eq!(b.ours, "");
        assert_eq!(b.base.as_deref(), Some("old\n"));
        assert_eq!(b.theirs, "new\n");
        assert_eq!(render_segments(&segments), text);
    }

    #[test]
    fn crlf_and_no_final_newline() {
        let text = "a\r\n<<<<<<< HEAD\r\nours\r\n=======\r\ntheirs\r\n>>>>>>> b\r\nend";
        let segments = parse_conflicts(text, 7).expect("parse");
        let b = blocks(&segments)[0];
        assert_eq!(b.ours, "ours\r\n");
        assert_eq!(b.separator, "=======\r\n");
        assert_eq!(render_segments(&segments), text);

        let last = "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> b";
        let segments = parse_conflicts(last, 7).expect("parse");
        assert_eq!(blocks(&segments)[0].theirs_marker, ">>>>>>> b");
        assert_eq!(render_segments(&segments), last);
    }

    #[test]
    fn lookalike_lines_are_text() {
        // Longer runs, a label on the separator and markers outside a block.
        let text = "Title\n=======\n<<<<<<<< not\n>>>>>>> stray\n======= x\n";
        let segments = parse_conflicts(text, 7).expect("parse");
        assert_eq!(conflict_count(&segments), 0);
        assert_eq!(render_segments(&segments), text);
    }

    #[test]
    fn longer_runs_inside_a_block_are_content() {
        let text = "<<<<<<< HEAD\n========\n=======\n>>>>>>>>\n>>>>>>> b\n";
        let segments = parse_conflicts(text, 7).expect("parse");
        let b = blocks(&segments)[0];
        assert_eq!(b.ours, "========\n");
        assert_eq!(b.theirs, ">>>>>>>>\n");
    }

    #[test]
    fn custom_marker_size() {
        let text = "<<<<<<<<<< HEAD\n<<<<<<< kept\n==========\nx\n>>>>>>>>>> b\n";
        let segments = parse_conflicts(text, 10).expect("parse");
        let b = blocks(&segments)[0];
        assert_eq!(b.ours, "<<<<<<< kept\n");
        assert_eq!(render_segments(&segments), text);
        // With the default size, the same file is plain text plus a stray
        // opening marker that is never closed.
        assert_eq!(parse_conflicts(text, 7), Err(MalformedMarkers { line: 2 }));
    }

    #[test]
    fn malformed_markers() {
        let unclosed = "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n";
        assert_eq!(
            parse_conflicts(unclosed, 7),
            Err(MalformedMarkers { line: 2 })
        );
        let nested = "<<<<<<< HEAD\n<<<<<<< again\n";
        assert_eq!(
            parse_conflicts(nested, 7),
            Err(MalformedMarkers { line: 2 })
        );
        let no_separator = "<<<<<<< HEAD\nours\n>>>>>>> b\n";
        assert_eq!(
            parse_conflicts(no_separator, 7),
            Err(MalformedMarkers { line: 3 })
        );
        let base_after_separator = "<<<<<<< HEAD\n=======\n||||||| base\n>>>>>>> b\n";
        assert_eq!(
            parse_conflicts(base_after_separator, 7),
            Err(MalformedMarkers { line: 3 })
        );
    }

    #[test]
    fn marker_lines_anywhere() {
        let text = "ok\n<<<<<<< HEAD\r\nx\n=======\n";
        assert_eq!(
            marker_lines(text, 7),
            vec![(2, "<<<<<<< HEAD".to_owned()), (4, "=======".to_owned())]
        );
    }

    /// Files left by `git merge` with each `merge.conflictStyle` (the same
    /// three-commit history; the CRLF one stores its lines with CRLF).
    const MERGE: &str = include_str!("../../tests/fixtures/conflicts/merge-lf.txt");
    const DIFF3: &str = include_str!("../../tests/fixtures/conflicts/diff3-lf.txt");
    const ZDIFF3: &str = include_str!("../../tests/fixtures/conflicts/zdiff3-lf.txt");
    const DIFF3_CRLF: &str = include_str!("../../tests/fixtures/conflicts/diff3-crlf.txt");

    #[test]
    fn real_git_files_round_trip() {
        for (name, text, count) in [
            ("merge", MERGE, 2),
            ("diff3", DIFF3, 3),
            ("zdiff3", ZDIFF3, 3),
            ("diff3 crlf", DIFF3_CRLF, 3),
        ] {
            let segments = parse_conflicts(text, DEFAULT_MARKER_SIZE).expect(name);
            assert_eq!(conflict_count(&segments), count, "{name}");
            assert_eq!(render_segments(&segments), text, "{name}");
        }
    }

    #[test]
    fn real_git_files_blocks() {
        let merge = parse_conflicts(MERGE, 7).expect("merge");
        let first = blocks(&merge)[0];
        assert_eq!(first.base, None);
        assert_eq!(
            first.ours,
            "    let a = 100;\n    let b = 2;\n    let c = 300;\n"
        );
        assert_eq!(
            first.theirs,
            "    let a = 10;\n    let b = 2;\n    let c = 30;\n"
        );

        let diff3 = parse_conflicts(DIFF3, 7).expect("diff3");
        let first = blocks(&diff3)[0];
        assert!(
            first
                .base_marker
                .as_deref()
                .is_some_and(|m| m.starts_with("||||||| "))
        );
        assert_eq!(first.base.as_deref(), Some("    let a = 1;\n"));
        assert_eq!(
            blocks(&diff3)[1].base.as_deref(),
            Some("    let c = 3;\n    println!(\"{a}\");\n")
        );

        // zdiff3 moves the lines both sides share out of the block.
        let zdiff3 = parse_conflicts(ZDIFF3, 7).expect("zdiff3");
        assert_eq!(blocks(&zdiff3)[1].ours, "    let c = 300;\n");
        assert_eq!(
            blocks(&zdiff3)[1].base.as_deref(),
            Some("    let c = 3;\n    println!(\"{a}\");\n")
        );

        let crlf = parse_conflicts(DIFF3_CRLF, 7).expect("crlf");
        let last = blocks(&crlf)[2];
        assert_eq!(last.theirs, "// tail two (feature)\r\n");
        assert_eq!(last.theirs_marker, ">>>>>>> feature\r\n");
    }

    #[test]
    fn empty_text() {
        assert_eq!(parse_conflicts("", 7), Ok(vec![]));
    }
}
