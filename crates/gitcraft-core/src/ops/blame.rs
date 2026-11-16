//! Blame (task 12.5): which commit each line of a file comes from, at a
//! given commit. Read-only.
//!
//! The file's text is sent first, whole; then the groups of lines as git
//! attributes them (`--incremental`, in any order), in batches, with how
//! many lines are attributed so far. Binary files are refused, very big
//! ones too, and big ones only start once the user agrees (`force`).

use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use crate::ops::search::BATCH_INTERVAL;
use crate::parse::blame::{BlameChunk, BlameCommit, BlameEvent, BlameParser};
use crate::parse::log::is_object_id;
use crate::{CancellationToken, Git, GitError, Result};

/// More lines than this: ask before blaming (it can take a while).
pub const LARGE_LINES: u32 = 20_000;
/// More bytes than this: never blamed (nor shown).
pub const MAX_BYTES: u64 = 16 * 1024 * 1024;
/// How much of the start git looks at to call a file binary.
const BINARY_PEEK: usize = 8000;
/// A batch is sent with at least this many groups, or after
/// [`BATCH_INTERVAL`].
const BATCH_CHUNKS: usize = 200;

/// What the blame stream carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum BlameStream {
    /// The file at that commit, once, before any batch.
    Text {
        lines: Vec<String>,
    },
    Batch(BlameBatch),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BlameBatch {
    /// Commits seen for the first time.
    pub commits: Vec<BlameCommit>,
    pub chunks: Vec<BlameChunk>,
    /// Lines attributed so far, these included.
    pub attributed: u32,
}

/// How it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum BlameOutcome {
    Done {
        lines: u32,
    },
    /// The file is not in that commit.
    Missing,
    Binary,
    /// Over [`MAX_BYTES`].
    TooLarge {
        #[cfg_attr(feature = "specta", specta(type = f64))]
        bytes: u64,
    },
    /// Over [`LARGE_LINES`] and not `force`: nothing was blamed yet.
    Large {
        lines: u32,
    },
}

/// Blames `path` (its path in `rev`, `HEAD` or a commit id). Respects the
/// repository's `blame.ignoreRevsFile`. Cancelling stops git and returns
/// [`GitError::Cancelled`].
pub async fn blame(
    git: &Git,
    repo: &Path,
    path: &str,
    rev: &str,
    force: bool,
    cancel: &CancellationToken,
    mut on_event: impl FnMut(BlameStream),
) -> Result<BlameOutcome> {
    if rev != "HEAD" && !is_object_id(rev) {
        return Err(GitError::Parse(format!("blame: not a commit id: {rev:?}")));
    }
    let object = format!("{rev}:{path}");
    let kind = git
        .read(repo)
        .args(["cat-file", "-t"])
        .arg(&object)
        .accept_exit_code(128)
        .output(cancel)
        .await?;
    if kind.exit_code != Some(0) || kind.stdout_lossy().trim() != "blob" {
        return Ok(BlameOutcome::Missing);
    }
    let size = git
        .read(repo)
        .args(["cat-file", "-s"])
        .arg(&object)
        .output(cancel)
        .await?;
    let bytes: u64 = size.stdout_lossy().trim().parse().unwrap_or(0);
    if bytes > MAX_BYTES {
        return Ok(BlameOutcome::TooLarge { bytes });
    }
    let blob = git
        .read(repo)
        .args(["cat-file", "blob"])
        .arg(&object)
        .output(cancel)
        .await?;
    let content = &blob.stdout;
    if content[..content.len().min(BINARY_PEEK)].contains(&0) {
        return Ok(BlameOutcome::Binary);
    }
    let text = String::from_utf8_lossy(content);
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    if text.ends_with('\n') || text.is_empty() {
        lines.pop();
    }
    let total = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    if total > LARGE_LINES && !force {
        return Ok(BlameOutcome::Large { lines: total });
    }
    on_event(BlameStream::Text { lines });
    if total == 0 {
        return Ok(BlameOutcome::Done { lines: 0 });
    }

    let mut parser = BlameParser::default();
    let mut commits = Vec::new();
    let mut chunks = Vec::new();
    let mut attributed = 0u32;
    let mut last = Instant::now();
    let mut parse_error = None;
    let sink = |record: &[u8]| {
        if parse_error.is_some() {
            return;
        }
        let line = String::from_utf8_lossy(record);
        match parser.push(line.trim_end_matches('\r')) {
            Ok(events) => {
                for event in events {
                    match event {
                        BlameEvent::Commit(commit) => commits.push(commit),
                        BlameEvent::Chunk(chunk) => {
                            attributed += chunk.count;
                            chunks.push(chunk);
                        }
                    }
                }
            }
            Err(err) => parse_error = Some(err),
        }
        if !chunks.is_empty() && (chunks.len() >= BATCH_CHUNKS || last.elapsed() >= BATCH_INTERVAL)
        {
            last = Instant::now();
            on_event(BlameStream::Batch(BlameBatch {
                commits: std::mem::take(&mut commits),
                chunks: std::mem::take(&mut chunks),
                attributed,
            }));
        }
    };
    git.read(repo)
        .args(["blame", "--incremental", rev, "--", path])
        .stream(b'\n', cancel, sink, |_| {})
        .await?;
    if let Some(err) = parse_error {
        return Err(err);
    }
    if !chunks.is_empty() || !commits.is_empty() {
        on_event(BlameStream::Batch(BlameBatch {
            commits,
            chunks,
            attributed,
        }));
    }
    Ok(BlameOutcome::Done { lines: total })
}
