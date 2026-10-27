//! Commit-graph detection and writing.

use gitcraft_core::ops::maintenance::{has_commit_graph, write_commit_graph};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, Git};

fn git() -> Git {
    Git::discover().expect("git must be installed to run the tests")
}

#[tokio::test]
async fn writes_and_detects_a_commit_graph() {
    let (repo, _) = TestRepo::sample_history().await;
    let cancel = CancellationToken::new();
    assert!(
        !has_commit_graph(&git(), repo.path(), &cancel)
            .await
            .expect("check")
    );

    let mut progress = Vec::new();
    write_commit_graph(&git(), repo.path(), &cancel, |line| {
        progress.push(line.to_owned())
    })
    .await
    .expect("write");

    assert!(
        has_commit_graph(&git(), repo.path(), &cancel)
            .await
            .expect("check")
    );
    assert!(
        progress.iter().any(|l| l.contains("commit graph")),
        "git reports progress: {progress:?}"
    );
    // The Bloom filters were requested too.
    let verify = repo.run(&["commit-graph", "verify"]).await;
    assert!(verify.is_empty(), "{verify}");
}

#[tokio::test]
async fn detects_it_from_a_subdirectory() {
    let (repo, _) = TestRepo::sample_history().await;
    repo.write_file("sub/x.txt", "x");
    let cancel = CancellationToken::new();
    write_commit_graph(&git(), repo.path(), &cancel, |_| {})
        .await
        .expect("write");
    assert!(
        has_commit_graph(&git(), &repo.path().join("sub"), &cancel)
            .await
            .expect("check")
    );
}
