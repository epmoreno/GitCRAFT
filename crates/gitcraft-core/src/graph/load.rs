//! Streams the commit graph in pages: commits are parsed and laid out as git
//! produces them, so the first rows reach the UI before the log finishes.

use std::path::Path;

use serde::Serialize;

use super::layout::{GraphLayout, GraphRow, Lane};
use crate::ops::log::stream_commits;
use crate::parse::log::Commit;
use crate::{CancellationToken, Git, Result};

/// The first page is small so the first screenful appears quickly.
pub const FIRST_PAGE_SIZE: usize = 256;
pub const PAGE_SIZE: usize = 2000;

/// `[column, colour]`. A pair instead of an object keeps wide graphs small
/// on the wire (every row repeats its passing lines).
pub type WireLane = (u16, u32);

/// Exactly what the history list needs to draw one row. Everything else
/// (full message, committer, parents, files) is loaded when a commit is
/// selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GraphLine {
    pub oid: String,
    pub summary: String,
    pub author: String,
    /// Author date, Unix seconds.
    #[cfg_attr(feature = "specta", specta(type = i32))]
    pub time: i64,
    /// Drawn as a hollow node.
    pub merge: bool,
    pub column: u16,
    pub color: u32,
    pub incoming: Vec<WireLane>,
    pub outgoing: Vec<WireLane>,
    pub passing: Vec<WireLane>,
}

impl GraphLine {
    pub fn new(commit: Commit, row: GraphRow) -> Self {
        let wire = |lanes: Vec<Lane>| lanes.into_iter().map(|l| (l.column, l.color)).collect();
        Self {
            merge: commit.is_merge(),
            oid: commit.oid,
            summary: commit.summary,
            author: commit.author_name,
            time: commit.author_time,
            column: row.column,
            color: row.color,
            incoming: wire(row.incoming),
            outgoing: wire(row.outgoing),
            passing: wire(row.passing),
        }
    }
}

/// Loads the whole graph, handing over pages of laid-out commits in order.
/// Returns the number of commits.
pub async fn load_graph(
    git: &Git,
    repo: &Path,
    include_head: bool,
    cancel: &CancellationToken,
    mut on_page: impl FnMut(Vec<GraphLine>),
) -> Result<usize> {
    let mut layout = GraphLayout::new();
    let mut page = Vec::with_capacity(FIRST_PAGE_SIZE);
    let mut limit = FIRST_PAGE_SIZE;
    let mut total = 0;
    stream_commits(git, repo, include_head, cancel, |commit| {
        let row = layout.push(&commit.oid, &commit.parents);
        page.push(GraphLine::new(commit, row));
        total += 1;
        if page.len() >= limit {
            limit = PAGE_SIZE;
            on_page(std::mem::replace(&mut page, Vec::with_capacity(limit)));
        }
    })
    .await?;
    if !page.is_empty() {
        on_page(page);
    }
    Ok(total)
}
