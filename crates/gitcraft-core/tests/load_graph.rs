//! Paged graph loading.

use std::collections::BTreeMap;

use gitcraft_core::graph::{GraphEntry, load_graph};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, Git, GitError};

fn git() -> Git {
    Git::discover().expect("git must be installed to run the tests")
}

async fn pages(repo: &TestRepo) -> Vec<Vec<GraphEntry>> {
    let mut pages = Vec::new();
    let total = load_graph(&git(), repo.path(), false, &CancellationToken::new(), |p| {
        pages.push(p)
    })
    .await
    .expect("load");
    assert_eq!(total, pages.iter().map(Vec::len).sum::<usize>());
    pages
}

#[tokio::test]
async fn small_histories_arrive_in_one_page() {
    let (repo, ids) = TestRepo::sample_history().await;
    let pages = pages(&repo).await;
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].len(), 7);
    assert_eq!(pages[0][0].commit.oid, ids["E"]);
}

#[tokio::test]
async fn an_empty_repository_sends_no_pages() {
    let repo = TestRepo::new().await;
    assert!(pages(&repo).await.is_empty());
}

#[tokio::test]
async fn large_histories_are_paged_and_stay_continuous() {
    let repo = TestRepo::synthetic(3000).await;
    let pages = pages(&repo).await;
    let sizes: Vec<usize> = pages.iter().map(Vec::len).collect();
    assert_eq!(sizes, [256, 2000, 744]);

    // Lines continue across page boundaries exactly as within a page.
    let entries: Vec<&GraphEntry> = pages.iter().flatten().collect();
    for pair in entries.windows(2) {
        let bottom: BTreeMap<u16, u32> = pair[0]
            .row
            .outgoing
            .iter()
            .chain(&pair[0].row.passing)
            .map(|l| (l.column, l.color))
            .collect();
        let top: BTreeMap<u16, u32> = pair[1]
            .row
            .incoming
            .iter()
            .chain(&pair[1].row.passing)
            .map(|l| (l.column, l.color))
            .collect();
        assert_eq!(bottom, top, "at {}", pair[1].commit.oid);
    }
    // The synthetic history has merges and parallel branches.
    assert!(entries.iter().any(|e| e.commit.parents.len() == 2));
    assert!(entries.iter().any(|e| e.row.column > 0));
}

#[tokio::test]
async fn loading_can_be_cancelled_between_pages() {
    let repo = TestRepo::synthetic(5000).await;
    let cancel = CancellationToken::new();
    let mut received = 0;
    let result = load_graph(&git(), repo.path(), false, &cancel, |page| {
        received += page.len();
        cancel.cancel();
    })
    .await;
    assert!(matches!(result, Err(GitError::Cancelled)), "{result:?}");
    assert!(received < 5000);
}
