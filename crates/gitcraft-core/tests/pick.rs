//! Cherry-pick against real repositories: nothing is touched until the
//! user accepts, and conflicts end in task 9's resolution.

use gitcraft_core::CancellationToken;
use gitcraft_core::ops::branch::BlockingFile;
use gitcraft_core::ops::in_progress::{ContinueOutcome, continue_in_progress};
use gitcraft_core::ops::pick::{
    PickBlock, PickOutcome, PickPreview, PickRefusal, cherry_pick, cherry_pick_preview,
    pick_stash_message,
};
use gitcraft_core::testing::TestRepo;

fn c() -> CancellationToken {
    CancellationToken::new()
}

/// `topic` has "topic edits f" (f: base → topic) and "topic adds new.txt";
/// `main` has its own commit on other.txt.
async fn repo_with_topic() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.write_file("f.txt", "base\n");
    repo.write_file("other.txt", "other\n");
    repo.commit_all("base").await;
    repo.create_branch("topic").await;
    repo.checkout("topic").await;
    repo.commit_file("f.txt", "topic\n", "topic edits f").await;
    repo.commit_file("new.txt", "new\n", "topic adds new.txt")
        .await;
    repo.checkout("main").await;
    repo.commit_file("other.txt", "other (main)\n", "main edits other")
        .await;
    repo
}

async fn preview(repo: &TestRepo, oid: &str) -> PickPreview {
    cherry_pick_preview(repo.git(), repo.path(), oid, &c())
        .await
        .expect("preview")
}

async fn pick(repo: &TestRepo, oid: &str, save: bool, allow: bool) -> PickOutcome {
    cherry_pick(repo.git(), repo.path(), oid, save, allow, &c())
        .await
        .expect("cherry_pick")
}

fn read(repo: &TestRepo, path: &str) -> String {
    std::fs::read_to_string(repo.path().join(path)).expect("read")
}

#[tokio::test]
async fn a_clean_pick_copies_the_commit() {
    let repo = repo_with_topic().await;
    let oid = repo.rev_parse("topic~1").await;
    let p = preview(&repo, &oid).await;
    assert_eq!(p.commit.subject, "topic edits f");
    assert_eq!(p.branch.as_deref(), Some("main"));
    assert_eq!((p.refused, p.blocked), (None, None));
    assert!(p.conflicts.is_empty() && p.exact);

    let PickOutcome::Done { head, saved } = pick(&repo, &oid, false, false).await else {
        panic!("expected a clean pick");
    };
    assert_eq!(head, repo.head().await);
    assert_eq!(saved, None);
    assert_eq!(
        repo.run(&["log", "-1", "--format=%s"]).await,
        "topic edits f"
    );
    assert_eq!(read(&repo, "f.txt"), "topic\n");
    // The original commit is untouched; main got a copy.
    assert_ne!(head, oid);
}

#[tokio::test]
async fn conflicts_are_announced_then_resolved_with_task_9() {
    let repo = repo_with_topic().await;
    repo.commit_file("f.txt", "main\n", "main edits f").await;
    let head = repo.head().await;
    let oid = repo.rev_parse("topic~1").await;

    let p = preview(&repo, &oid).await;
    assert_eq!(p.conflicts, vec!["f.txt".to_owned()]);
    // Not accepted: nothing touched.
    assert!(matches!(
        pick(&repo, &oid, false, false).await,
        PickOutcome::NotDone { .. }
    ));
    assert_eq!(repo.head().await, head);
    assert_eq!(read(&repo, "f.txt"), "main\n");

    assert_eq!(
        pick(&repo, &oid, false, true).await,
        PickOutcome::Conflicted {
            files: vec!["f.txt".into()],
            saved: None
        }
    );
    repo.write_file("f.txt", "main + topic\n");
    repo.run(&["add", "f.txt"]).await;
    assert_eq!(
        continue_in_progress(repo.git(), repo.path(), None, &c())
            .await
            .expect("continue"),
        ContinueOutcome::Done
    );
    assert_eq!(
        repo.run(&["log", "-1", "--format=%s"]).await,
        "topic edits f"
    );
    assert_eq!(read(&repo, "f.txt"), "main + topic\n");
}

#[tokio::test]
async fn staged_changes_anywhere_are_in_the_way() {
    let repo = repo_with_topic().await;
    let oid = repo.rev_parse("topic~1").await;
    repo.write_file("other.txt", "staged\n");
    repo.run(&["add", "other.txt"]).await;
    let p = preview(&repo, &oid).await;
    assert_eq!(p.blocked, Some(PickBlock::StagedChanges));
    assert_eq!(
        p.blocking,
        vec![BlockingFile {
            path: "other.txt".into(),
            untracked: false
        }]
    );
    let head = repo.head().await;
    assert!(matches!(
        pick(&repo, &oid, false, false).await,
        PickOutcome::NotDone { .. }
    ));
    assert_eq!(repo.head().await, head);

    // "Save my changes, then cherry-pick": a stash entry that stays, named
    // in the outcome so the app can offer it back.
    let PickOutcome::Done { saved, .. } = pick(&repo, &oid, true, false).await else {
        panic!("expected a pick");
    };
    assert_eq!(saved, Some(repo.rev_parse("stash@{0}").await));
    let short = &oid[..7];
    assert_eq!(
        repo.run(&["stash", "list", "--format=%gs"]).await,
        format!("On main: {}", pick_stash_message(short))
    );
    assert_eq!(repo.run(&["show", "stash@{0}:other.txt"]).await, "staged");
    assert_eq!(read(&repo, "other.txt"), "other (main)\n");
}

#[tokio::test]
async fn unstaged_changes_only_matter_in_the_commits_files() {
    let repo = repo_with_topic().await;
    let oid = repo.rev_parse("topic~1").await;
    repo.write_file("other.txt", "mine\n");
    assert_eq!(preview(&repo, &oid).await.blocked, None);
    assert!(matches!(
        pick(&repo, &oid, false, false).await,
        PickOutcome::Done { .. }
    ));
    assert_eq!(read(&repo, "other.txt"), "mine\n", "kept as it was");

    let repo = repo_with_topic().await;
    repo.write_file("f.txt", "mine\n");
    repo.write_file("new.txt", "untracked, in the way\n");
    let p = preview(&repo, &repo.rev_parse("topic~1").await).await;
    assert_eq!(p.blocked, Some(PickBlock::Overwrite));
    assert_eq!(p.blocking[0].path, "f.txt");
    let adds = preview(&repo, &repo.rev_parse("topic").await).await;
    assert_eq!(
        adds.blocking,
        vec![BlockingFile {
            path: "new.txt".into(),
            untracked: true
        }]
    );
}

#[tokio::test]
async fn refusals() {
    let (repo, ids) = TestRepo::sample_history().await;
    // Already in main, a merge, then a detached HEAD.
    assert_eq!(
        preview(&repo, &ids["B"]).await.refused,
        Some(PickRefusal::AlreadyInBranch)
    );
    assert_eq!(
        preview(&repo, &ids["M"]).await.refused,
        Some(PickRefusal::Merge)
    );
    repo.run(&["switch", "--quiet", "--detach", &ids["B"]])
        .await;
    assert_eq!(
        preview(&repo, &ids["F"]).await.refused,
        Some(PickRefusal::Detached)
    );
    assert!(matches!(
        pick(&repo, &ids["F"], true, true).await,
        PickOutcome::NotDone { .. }
    ));

    let repo = repo_with_topic().await;
    repo.commit_file("f.txt", "main\n", "main edits f").await;
    let oid = repo.rev_parse("topic~1").await;
    pick(&repo, &oid, false, true).await;
    assert_eq!(
        preview(&repo, &repo.rev_parse("topic").await).await.refused,
        Some(PickRefusal::InProgress)
    );
}

#[tokio::test]
async fn a_root_commit_can_be_picked() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "first").await;
    // An unrelated history: `--orphan` starts with nothing tracked.
    repo.run(&["switch", "--quiet", "--orphan", "other"]).await;
    repo.commit_file("b.txt", "b\n", "other root").await;
    let root = repo.head().await;
    repo.checkout("main").await;
    let p = preview(&repo, &root).await;
    assert_eq!((p.refused, p.blocked), (None, None));
    assert!(matches!(
        pick(&repo, &root, false, false).await,
        PickOutcome::Done { .. }
    ));
    assert_eq!(read(&repo, "b.txt"), "b\n");
}
