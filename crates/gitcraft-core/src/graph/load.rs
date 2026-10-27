//! Streams the commit graph in pages: commits are parsed and laid out as git
//! produces them, so the first rows reach the UI before the log finishes.

use std::path::Path;

use serde::Serialize;

use super::layout::{GraphLayout, GraphRow};
use crate::ops::log::stream_commits;
use crate::parse::log::Commit;
use crate::{CancellationToken, Git, Result};

/// The first page is small so the first screenful appears quickly.
pub const FIRST_PAGE_SIZE: usize = 256;
pub const PAGE_SIZE: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GraphEntry {
    pub commit: Commit,
    pub row: GraphRow,
}

/// Loads the whole graph, handing over pages of laid-out commits in order.
/// Returns the number of commits.
pub async fn load_graph(
    git: &Git,
    repo: &Path,
    include_head: bool,
    cancel: &CancellationToken,
    mut on_page: impl FnMut(Vec<GraphEntry>),
) -> Result<usize> {
    let mut layout = GraphLayout::new();
    let mut page = Vec::with_capacity(FIRST_PAGE_SIZE);
    let mut limit = FIRST_PAGE_SIZE;
    let mut total = 0;
    stream_commits(git, repo, include_head, cancel, |commit| {
        let row = layout.push(&commit.oid, &commit.parents);
        page.push(GraphEntry { commit, row });
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
