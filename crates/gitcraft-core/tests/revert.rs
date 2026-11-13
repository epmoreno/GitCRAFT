//! Revert against real repositories: a new commit undoes one of the
//! branch's commits; nothing is touched until the user accepts.

use gitcraft_core::CancellationToken;
use gitcraft_core::ops::in_progress::{ContinueOutcome, continue_in_progress};
use gitcraft_core::ops::pick::{
    PickBlock, PickKind, PickOutcome, PickPreview, PickRefusal, revert, revert_preview,
    revert_stash_message,
};
use gitcraft_core::testing::TestRepo;

fn c() -> CancellationToken {
    CancellationToken::new()
}

/// main: base (f = 1, g = a) → "edit f" (f = 2) → "edit g" (g = b).
async fn history() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.write_file("f.txt", "1\n");
    repo.write_file("g.txt", "a\n");
    repo.commit_all("base").await;
    repo.commit_file("f.txt", "2\n", "edit f").await;
    repo.commit_file("g.txt", "b\n", "edit g").await;
    repo
}

async fn preview(repo: &TestRepo, oid: &str) -> PickPreview {
    revert_preview(repo.git(), repo.path(), oid, &c())
        .await
        .expect("preview")
}

async fn run(repo: &TestRepo, oid: &str, save: bool, allow: bool) -> PickOutcome {
    revert(repo.git(), repo.path(), oid, save, allow, &c())
        .await
        .expect("revert")
}

fn read(repo: &TestRepo, path: &str) -> String {
    std::fs::read_to_string(repo.path().join(path)).expect("read")
}

#[tokio::test]
async fn a_new_commit_undoes_the_old_one() {
    let repo = history().await;
    let oid = repo.rev_parse("HEAD~1").await;
    let p = preview(&repo, &oid).await;
    assert_eq!(p.kind, PickKind::Revert);
    assert_eq!(p.commit.subject, "edit f");
    assert_eq!((p.refused, p.blocked), (None, None));
    assert!(p.conflicts.is_empty());

    let PickOutcome::Done { head, saved } = run(&repo, &oid, false, false).await else {
        panic!("expected a revert");
    };
    assert_eq!(saved, None);
    assert_eq!(head, repo.head().await);
    assert_eq!(
        repo.run(&["log", "-1", "--format=%s"]).await,
        "Revert \"edit f\""
    );
    assert_eq!(read(&repo, "f.txt"), "1\n");
    assert_eq!(read(&repo, "g.txt"), "b\n", "later commits stay");
    // The reverted commit is still in the history.
    assert_eq!(
        repo.run(&["log", "--format=%s", "-4"])
            .await
            .lines()
            .count(),
        4
    );
}

#[tokio::test]
async fn conflicts_are_announced_then_resolved_with_task_9() {
    let repo = history().await;
    repo.commit_file("f.txt", "3\n", "edit f again").await;
    let head = repo.head().await;
    let oid = repo.rev_parse("HEAD~2").await;
    assert_eq!(
        preview(&repo, &oid).await.conflicts,
        vec!["f.txt".to_owned()]
    );
    assert!(matches!(
        run(&repo, &oid, false, false).await,
        PickOutcome::NotDone { .. }
    ));
    assert_eq!(repo.head().await, head);

    assert!(matches!(
        run(&repo, &oid, false, true).await,
        PickOutcome::Conflicted { .. }
    ));
    repo.write_file("f.txt", "1 and 3\n");
    repo.run(&["add", "f.txt"]).await;
    assert_eq!(
        continue_in_progress(repo.git(), repo.path(), None, &c())
            .await
            .expect("continue"),
        ContinueOutcome::Done
    );
    assert_eq!(
        repo.run(&["log", "-1", "--format=%s"]).await,
        "Revert \"edit f\""
    );
}

#[tokio::test]
async fn changes_in_the_way_can_be_saved_first() {
    let repo = history().await;
    let oid = repo.rev_parse("HEAD~1").await;
    repo.write_file("f.txt", "mine\n");
    let p = preview(&repo, &oid).await;
    assert_eq!(p.blocked, Some(PickBlock::Overwrite));
    let PickOutcome::Done { saved, .. } = run(&repo, &oid, true, false).await else {
        panic!("expected a revert");
    };
    assert_eq!(saved, Some(repo.rev_parse("stash@{0}").await));
    assert_eq!(
        repo.run(&["stash", "list", "--format=%gs"]).await,
        format!("On main: {}", revert_stash_message(&oid[..7]))
    );
    assert_eq!(read(&repo, "f.txt"), "1\n");
}

#[tokio::test]
async fn refusals() {
    let (repo, ids) = TestRepo::sample_history().await;
    // F is on topic, not main; M is a merge.
    assert_eq!(
        preview(&repo, &ids["F"]).await.refused,
        Some(PickRefusal::NotInBranch)
    );
    assert_eq!(
        preview(&repo, &ids["M"]).await.refused,
        Some(PickRefusal::Merge)
    );
    assert!(matches!(
        run(&repo, &ids["F"], true, true).await,
        PickOutcome::NotDone { .. }
    ));
    assert_eq!(preview(&repo, &ids["B"]).await.refused, None);
}
