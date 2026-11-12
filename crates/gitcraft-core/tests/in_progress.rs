//! The two sides of a stopped merge, rebase, cherry-pick and revert, named
//! by what they are.

use gitcraft_core::CancellationToken;
use gitcraft_core::ops::in_progress::{ConflictSides, RebaseProgress, conflict_sides};
use gitcraft_core::testing::TestRepo;

fn c() -> CancellationToken {
    CancellationToken::new()
}

async fn stop_on_conflict(repo: &TestRepo, args: &[&str]) {
    let out = repo
        .write()
        .args(args)
        .accept_exit_code(1)
        .output(&c())
        .await
        .expect("git");
    assert_eq!(out.exit_code, Some(1), "{args:?} should stop on a conflict");
}

async fn sides(repo: &TestRepo) -> Option<ConflictSides> {
    conflict_sides(repo.git(), repo.path(), &c())
        .await
        .expect("conflict_sides")
}

/// `main` and `feature` both change `f.txt` after `base`.
async fn diverged() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    repo.commit_file("f.txt", "feature\n", "feature change")
        .await;
    repo.checkout("main").await;
    repo.commit_file("f.txt", "main\n", "main change").await;
    repo
}

#[tokio::test]
async fn nothing_in_progress() {
    let repo = diverged().await;
    assert_eq!(sides(&repo).await, None);
}

#[tokio::test]
async fn merge_names_the_branch_being_merged() {
    let repo = diverged().await;
    let main = repo.head().await;
    let feature = repo.rev_parse("feature").await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;

    let Some(ConflictSides::Merge {
        branch,
        head,
        merging,
    }) = sides(&repo).await
    else {
        panic!("expected a merge");
    };
    assert_eq!(branch.as_deref(), Some("main"));
    assert_eq!(head.oid, main);
    assert_eq!(head.subject, "main change");
    assert_eq!(merging.commit.oid, feature);
    assert_eq!(merging.commit.subject, "feature change");
    assert_eq!(merging.branches, vec!["feature".to_owned()]);
}

#[tokio::test]
async fn merging_a_commit_without_branch() {
    let repo = diverged().await;
    let feature = repo.rev_parse("feature").await;
    repo.run(&["branch", "-D", "feature"]).await;
    stop_on_conflict(&repo, &["merge", &feature]).await;

    let Some(ConflictSides::Merge { merging, .. }) = sides(&repo).await else {
        panic!("expected a merge");
    };
    assert_eq!(merging.commit.oid, feature);
    assert!(merging.branches.is_empty());
}

/// `topic` has three commits on top of `base`; the first and third
/// conflict with `main`.
async fn rebase_repo() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "base\n", "base").await;
    repo.create_branch("topic").await;
    repo.commit_file("a.txt", "main\n", "main change").await;
    repo.checkout("topic").await;
    repo.commit_file("a.txt", "topic 1\n", "topic one").await;
    repo.commit_file("b.txt", "b\n", "topic two").await;
    repo.commit_file("a.txt", "topic 3\n", "topic three").await;
    repo
}

async fn check_rebase(backend: &[&str]) {
    let repo = rebase_repo().await;
    let main = repo.rev_parse("main").await;
    let first = repo.rev_parse("topic~2").await;
    let mut args = vec!["rebase"];
    args.extend_from_slice(backend);
    args.push("main");
    stop_on_conflict(&repo, &args).await;

    let Some(ConflictSides::Rebase {
        branch,
        onto,
        head,
        applying,
        progress,
    }) = sides(&repo).await
    else {
        panic!("expected a rebase ({backend:?})");
    };
    assert_eq!(branch.as_deref(), Some("topic"));
    assert_eq!(onto.commit.oid, main);
    assert_eq!(onto.branches, vec!["main".to_owned()]);
    // Nothing replayed yet: HEAD is the new base.
    assert_eq!(head.oid, main);
    let applying = applying.expect("the commit being replayed");
    assert_eq!(applying.oid, first);
    assert_eq!(applying.subject, "topic one");
    assert_eq!(
        progress,
        Some(RebaseProgress {
            current: 1,
            total: 3
        }),
        "{backend:?}"
    );

    // Resolve, continue: it stops again at the third commit, on top of the
    // two already replayed.
    repo.write_file("a.txt", "resolved\n");
    repo.run(&["add", "a.txt"]).await;
    let out = repo
        .write()
        .env("GIT_EDITOR", "true")
        .args(["rebase", "--continue"])
        .accept_exit_code(1)
        .output(&c())
        .await
        .expect("rebase --continue");
    assert_eq!(out.exit_code, Some(1));
    let Some(ConflictSides::Rebase {
        head,
        applying,
        progress,
        ..
    }) = sides(&repo).await
    else {
        panic!("expected the rebase to stop again");
    };
    assert_eq!(head.subject, "topic two");
    assert_eq!(applying.map(|a| a.subject).as_deref(), Some("topic three"));
    assert_eq!(
        progress,
        Some(RebaseProgress {
            current: 3,
            total: 3
        })
    );
}

#[tokio::test]
async fn rebase_names_your_commit_as_the_one_being_replayed() {
    check_rebase(&[]).await;
}

#[tokio::test]
async fn rebase_with_the_apply_backend() {
    check_rebase(&["--apply"]).await;
}

#[tokio::test]
async fn cherry_pick_and_revert() {
    let repo = diverged().await;
    let feature = repo.rev_parse("feature").await;
    stop_on_conflict(&repo, &["cherry-pick", &feature]).await;
    let Some(ConflictSides::CherryPick {
        branch, picking, ..
    }) = sides(&repo).await
    else {
        panic!("expected a cherry-pick");
    };
    assert_eq!(branch.as_deref(), Some("main"));
    assert_eq!(picking.oid, feature);
    repo.run(&["cherry-pick", "--abort"]).await;

    // Reverting "base" conflicts with "main change", which edits the same
    // line.
    let base = repo.rev_parse("main~1").await;
    stop_on_conflict(&repo, &["revert", "--no-edit", &base]).await;
    let Some(ConflictSides::Revert { reverting, .. }) = sides(&repo).await else {
        panic!("expected a revert");
    };
    assert_eq!(reverting.oid, base);
    assert_eq!(reverting.subject, "base");
}
