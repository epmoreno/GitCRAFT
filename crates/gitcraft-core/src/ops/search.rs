//! Searching the history (task 12.1): the commits whose message, author,
//! id or files match, over the same commits as the graph. Read-only.
//!
//! Results arrive in batches as they are found, so they can be used from
//! the first one. A literal search of the message, the author or an id is
//! filtered here, which tells how far it has got (`examined`); a regular
//! expression, and a search by file, are filtered by git, which does not.

use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::parse::search::{
    AUTHOR_FORMAT, MESSAGE_FORMAT, OID_FORMAT, PATH_FORMAT, PathRecords, parse_search_record,
};
use crate::{CancellationToken, Git, Result};

/// A batch is sent once it has this many results…
pub const BATCH_RESULTS: usize = 100;
/// …or this long after the previous one, whatever comes first.
pub const BATCH_INTERVAL: Duration = Duration::from_millis(50);
/// Shortest id prefix searched (git's own minimum).
pub const MIN_HASH_LEN: usize = 4;

/// Where to look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SearchField {
    /// The whole message, subject and body.
    Message,
    /// The author's name and email.
    Author,
    /// The commit id, by prefix.
    Hash,
    /// The commits that changed a file whose path contains the text; when
    /// the text is a tracked file or folder, only that one.
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SearchQuery {
    pub text: String,
    pub field: SearchField,
    /// A regular expression (git's extended syntax) instead of literal
    /// text. Only for the message and the author; ignored otherwise.
    pub regex: bool,
    pub match_case: bool,
}

impl SearchQuery {
    /// Whether the batches say how many commits have been examined.
    pub fn tracks_progress(&self) -> bool {
        match self.field {
            SearchField::Message | SearchField::Author => !self.regex,
            SearchField::Hash => true,
            SearchField::Path => false,
        }
    }
}

/// Results found since the previous batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SearchBatch {
    /// In the order git walks the history (newest first, roughly).
    pub oids: Vec<String>,
    /// Results so far, these included.
    pub found: u32,
    /// Commits examined so far; 0 when git filters
    /// ([`SearchQuery::tracks_progress`]).
    pub examined: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SearchSummary {
    pub found: u32,
    pub examined: u32,
    /// Searching an id that no branch, tag or remote branch has: the
    /// commit it names, if it still exists (only in the branch history).
    pub outside: Option<String>,
}

/// Collects results and hands them over in batches.
struct Batcher<F: FnMut(SearchBatch)> {
    pending: Vec<String>,
    found: u32,
    examined: u32,
    sent_examined: u32,
    last: Instant,
    on_batch: F,
}

impl<F: FnMut(SearchBatch)> Batcher<F> {
    fn new(on_batch: F) -> Self {
        Self {
            pending: Vec::new(),
            found: 0,
            examined: 0,
            sent_examined: 0,
            last: Instant::now(),
            on_batch,
        }
    }

    fn examined(&mut self, hit: Option<&str>, counts: bool) {
        if counts {
            self.examined += 1;
        }
        if let Some(oid) = hit {
            self.found += 1;
            self.pending.push(oid.to_owned());
        }
        if self.pending.len() >= BATCH_RESULTS || self.last.elapsed() >= BATCH_INTERVAL {
            self.flush();
        }
    }

    /// Sends what is pending, or just the progress if it moved.
    fn flush(&mut self) {
        self.last = Instant::now();
        if self.pending.is_empty() && self.examined == self.sent_examined {
            return;
        }
        self.sent_examined = self.examined;
        (self.on_batch)(SearchBatch {
            oids: std::mem::take(&mut self.pending),
            found: self.found,
            examined: self.examined,
        });
    }
}

/// Literal text, ignoring case unless asked (Unicode-aware).
struct Needle {
    text: String,
    match_case: bool,
}

impl Needle {
    fn new(text: &str, match_case: bool) -> Self {
        Self {
            text: if match_case {
                text.to_owned()
            } else {
                text.to_lowercase()
            },
            match_case,
        }
    }

    fn found_in(&self, haystack: &str) -> bool {
        if self.match_case {
            haystack.contains(&self.text)
        } else {
            haystack.to_lowercase().contains(&self.text)
        }
    }
}

/// A pathspec for "the path contains `text`": `*` crosses `/` (no `glob`
/// magic), git's wildcards in the text are escaped, and it is relative to
/// the top of the repository.
pub fn path_pathspec(text: &str, match_case: bool) -> String {
    let magic = if match_case { "top" } else { "top,icase" };
    format!(":({magic})*{}*", escape_wildcards(text))
}

/// `text` with git's pathspec wildcards escaped.
fn escape_wildcards(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 4);
    for c in text.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// The tracked file or folder `text` names (spelled as in the index, which
/// matters without `match_case`), if any. A trailing `/` is ignored.
pub async fn exact_path(
    git: &Git,
    repo: &Path,
    text: &str,
    match_case: bool,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let text = text.trim().trim_end_matches('/');
    if text.is_empty() || text.starts_with('/') {
        return Ok(None);
    }
    let spec = if match_case {
        format!(":(top,literal){text}")
    } else {
        format!(":(top,icase){}", escape_wildcards(text))
    };
    // The index lists the file itself, or the files in the folder.
    let out = git
        .read(repo)
        .args(["ls-files", "-z", "--"])
        .arg(spec)
        .output(cancel)
        .await?;
    let listing = out.stdout_lossy();
    let wanted = text.chars().count();
    Ok(listing.split('\0').find_map(|entry| {
        let name: String = entry.chars().take(wanted).collect();
        let rest = &entry[name.len()..];
        let same = if match_case {
            name == text
        } else {
            name.to_lowercase() == text.to_lowercase()
        };
        (same && (rest.is_empty() || rest.starts_with('/'))).then_some(name)
    }))
}

/// A valid id prefix: hex digits, at least [`MIN_HASH_LEN`], lowercased.
pub fn hash_prefix(text: &str) -> Option<String> {
    let text = text.trim();
    (text.len() >= MIN_HASH_LEN && text.len() <= 64 && text.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| text.to_ascii_lowercase())
}

/// The commits of the graph: every branch, remote branch and tag, and a
/// detached HEAD (`include_head`).
fn graph_revisions(include_head: bool) -> Vec<&'static str> {
    let mut args = vec!["--branches", "--remotes", "--tags"];
    if include_head {
        args.push("HEAD");
    }
    args
}

/// Searches the history and hands over the results in batches, as they
/// are found. An empty text, or an id that is not one, finds nothing and
/// runs nothing. Cancelling stops git and returns
/// [`crate::GitError::Cancelled`]; a regular expression git cannot read
/// fails with [`crate::ErrorKind::InvalidPattern`].
pub async fn search(
    git: &Git,
    repo: &Path,
    query: &SearchQuery,
    include_head: bool,
    cancel: &CancellationToken,
    on_batch: impl FnMut(SearchBatch),
) -> Result<SearchSummary> {
    let empty = SearchSummary {
        found: 0,
        examined: 0,
        outside: None,
    };
    let text = query.text.trim();
    if text.is_empty() {
        return Ok(empty);
    }
    let hash = match query.field {
        SearchField::Hash => match hash_prefix(text) {
            Some(prefix) => Some(prefix),
            None => return Ok(empty),
        },
        _ => None,
    };
    let mut args: Vec<String> = vec!["log".into(), "-z".into()];
    // Filtered here (literal text), or by git.
    let needle = match (query.field, query.regex) {
        (SearchField::Message, false) => {
            args.push(MESSAGE_FORMAT.into());
            Some(Needle::new(text, query.match_case))
        }
        (SearchField::Author, false) => {
            args.push(AUTHOR_FORMAT.into());
            Some(Needle::new(text, query.match_case))
        }
        (SearchField::Hash, _) => {
            args.push(OID_FORMAT.into());
            None
        }
        (SearchField::Message | SearchField::Author, true) => {
            args.push(OID_FORMAT.into());
            args.push("--extended-regexp".into());
            if !query.match_case {
                args.push("--regexp-ignore-case".into());
            }
            let option = if query.field == SearchField::Message {
                "--grep="
            } else {
                "--author="
            };
            args.push(format!("{option}{text}"));
            None
        }
        (SearchField::Path, _) => {
            args.push(PATH_FORMAT.into());
            // Every commit that changed it, also on merged side branches;
            // with the files each one changes (`-c`: for a merge, only
            // those that differ from every parent), so that a merge that
            // only brings a change from one side is left out, as in the
            // history of a file.
            args.extend(["--full-history", "-c", "--name-only"].map(String::from));
            None
        }
    };
    args.extend(graph_revisions(include_head).into_iter().map(String::from));
    args.push("--".into());
    if query.field == SearchField::Path {
        // An exact path is much faster: git can use the commit-graph's
        // Bloom filters, which a wildcard pathspec rules out.
        let spec = match exact_path(git, repo, text, query.match_case, cancel).await? {
            Some(path) => format!(":(literal){path}"),
            None => path_pathspec(text, query.match_case),
        };
        args.push(spec);
    }

    let counts = query.tracks_progress();
    let mut batcher = Batcher::new(on_batch);
    let mut parse_error = None;
    let mut by_path = (query.field == SearchField::Path).then(PathRecords::default);
    let sink = |record: &[u8]| {
        if parse_error.is_some() || record.is_empty() {
            return;
        }
        let record = String::from_utf8_lossy(record);
        if let Some(paths) = by_path.as_mut() {
            match paths.push(&record) {
                Ok(Some(oid)) => batcher.examined(Some(&oid), counts),
                Ok(None) => {}
                Err(err) => parse_error = Some(err),
            }
            return;
        }
        let parsed = match parse_search_record(&record) {
            Ok(parsed) => parsed,
            Err(err) => {
                parse_error = Some(err);
                return;
            }
        };
        let hit = match (&needle, &hash) {
            (Some(needle), _) => needle.found_in(parsed.text),
            (None, Some(prefix)) => parsed.oid.starts_with(prefix.as_str()),
            (None, None) => true,
        };
        batcher.examined(hit.then_some(parsed.oid), counts);
    };
    git.read(repo)
        .args(args)
        .stream(0, cancel, sink, |_| {})
        .await?;
    if let Some(err) = parse_error {
        return Err(err);
    }
    if let Some(oid) = by_path.as_mut().and_then(PathRecords::finish) {
        batcher.examined(Some(&oid), counts);
    }
    batcher.flush();
    let mut summary = SearchSummary {
        found: batcher.found,
        examined: batcher.examined,
        outside: None,
    };
    if let (Some(prefix), 0) = (&hash, summary.found) {
        summary.outside = commit_named(git, repo, prefix, cancel).await?;
    }
    Ok(summary)
}

/// The commit an id prefix names, if exactly one does.
async fn commit_named(
    git: &Git,
    repo: &Path,
    prefix: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("{prefix}^{{commit}}"))
        .accept_exit_code(1)
        .accept_exit_code(128)
        .output(cancel)
        .await?;
    let oid = out.stdout_lossy().trim().to_owned();
    Ok((out.exit_code == Some(0) && !oid.is_empty()).then_some(oid))
}
