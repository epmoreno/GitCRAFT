//! The facts that decide which actions a commit of the graph offers.

use gitcraft_core::CancellationToken;
use gitcraft_core::ops::commit_actions::{CommitActions, commit_actions};
use gitcraft_core::parse::status::InProgress;
use gitcraft_core::testing::TestRepo;

async fn actions(repo: &TestRepo, oid: &str) -> CommitActions {
    commit_actions(repo.git(), repo.path(), oid, &CancellationToken::new())
        .await
        .expect("commit_actions")
}

#[tokio::test]
async fn commits_in_and_out_of_the_current_branch() {
    let (repo, ids) = TestRepo::sample_history().await;
    // On main: E is HEAD, B is in the branch, F (topic) is not, M a merge.
    let head = actions(&repo, &ids["E"]).await;
    assert_eq!(head.branch.as_deref(), Some("main"));
    assert!(head.is_head && head.in_branch && !head.is_merge);
    assert_eq!(head.in_progress, None);

    let older = actions(&repo, &ids["B"]).await;
    assert!(older.in_branch && !older.is_head);

    let other = actions(&repo, &ids["F"]).await;
    assert!(!other.in_branch && !other.is_head && !other.is_merge);

    let merge = actions(&repo, &ids["M"]).await;
    assert!(merge.is_merge && merge.in_branch);

    // A short id is accepted and returned whole.
    assert_eq!(actions(&repo, &ids["F"][..10]).await.oid, ids["F"]);
}

#[tokio::test]
async fn detached_head_and_an_operation_in_progress() {
    let (repo, ids) = TestRepo::sample_history().await;
    repo.run(&["switch", "--quiet", "--detach", &ids["B"]])
        .await;
    assert_eq!(actions(&repo, &ids["A"]).await.branch, None);

    repo.checkout("main").await;
    repo.write_file("main.txt", "conflict\n");
    repo.commit_all("main again").await;
    repo.checkout("feature").await;
    repo.write_file("main.txt", "other\n");
    repo.commit_all("feature edits main.txt").await;
    repo.checkout("main").await;
    let out = repo
        .write()
        .args(["merge", "feature"])
        .accept_exit_code(1)
        .output(&CancellationToken::new())
        .await
        .expect("merge");
    assert_eq!(out.exit_code, Some(1));
    assert_eq!(
        actions(&repo, &ids["A"]).await.in_progress,
        Some(InProgress::Merge)
    );
}

#[tokio::test]
async fn ids_that_are_not_commits_are_refused() {
    let (repo, _) = TestRepo::sample_history().await;
    for bad in ["", "--all", "HEAD", "zzzz", "abc"] {
        assert!(
            commit_actions(repo.git(), repo.path(), bad, &CancellationToken::new())
                .await
                .is_err(),
            "{bad:?}"
        );
    }
}
