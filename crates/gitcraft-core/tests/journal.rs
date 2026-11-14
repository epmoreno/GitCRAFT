//! The journal of operations (task 11.1), against real repositories: every
//! operation that can be undone returns a record with exactly the refs it
//! moved, and the ones that cannot (staging, stash, fetch, push…) return
//! nothing.

use gitcraft_core::CancellationToken;
use gitcraft_core::ops::branch::{
    CheckoutOutcome, checkout_branch, create_branch, delete_branch, rename_branch,
};
use gitcraft_core::ops::commit_write::create_commit;
use gitcraft_core::ops::journal::{
    HeadAt, OpKind, OpRecord, RefChange, Saved, Snapshot, current_fingerprint, current_refs,
    record, snapshot,
};
use gitcraft_core::ops::pick::{PickOutcome, cherry_pick, revert};
use gitcraft_core::ops::remote::{PullMode, PullOutcome, fetch, pull, push};
use gitcraft_core::ops::stage::stage;
use gitcraft_core::ops::stash::create_stash;
use gitcraft_core::ops::tag::{create_tag, delete_tag};
use gitcraft_core::testing::TestRepo;

fn c() -> CancellationToken {
    CancellationToken::new()
}

fn heads(name: &str) -> String {
    format!("refs/heads/{name}")
}

fn on(name: &str) -> HeadAt {
    HeadAt::Branch { name: heads(name) }
}

fn moved(refname: &str, before: Option<&str>, after: Option<&str>) -> RefChange {
    RefChange {
        refname: refname.to_owned(),
        before: before.map(str::to_owned),
        after: after.map(str::to_owned),
    }
}

async fn before(repo: &TestRepo) -> Snapshot {
    snapshot(repo.git(), repo.path(), &c())
        .await
        .expect("snapshot")
}

async fn after(repo: &TestRepo, before: Snapshot, kind: OpKind) -> Option<OpRecord> {
    record(
        repo.git(),
        repo.path(),
        before,
        kind,
        Saved::default(),
        &c(),
    )
    .await
    .expect("record")
}

fn summary(kind: &str) -> OpKind {
    match kind {
        "commit" => OpKind::Commit {
            summary: "x".into(),
        },
        _ => OpKind::Checkout {
            branch: kind.into(),
        },
    }
}

/// A repository with two commits on `main` and a branch `feature` with one
/// more, checked out on `main`.
async fn repo_with_feature() -> (TestRepo, String, String) {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "A").await;
    let main = repo.commit_file("b.txt", "b\n", "B").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    let feature = repo.commit_file("f.txt", "f\n", "F").await;
    repo.checkout("main").await;
    (repo, main, feature)
}

/// Branches, tags, HEAD and the status: what the snapshot must not change.
async fn visible_state(repo: &TestRepo) -> String {
    let refs = repo
        .run(&["for-each-ref", "--format=%(refname) %(objectname)"])
        .await;
    let head = repo.run(&["symbolic-ref", "-q", "HEAD"]).await;
    let status = repo
        .run(&["status", "--porcelain=v2", "--untracked-files=all"])
        .await;
    let index = repo.run(&["ls-files", "-s"]).await;
    format!("{refs}\n{head}\n{status}\n{index}")
}

#[tokio::test]
async fn a_commit_records_its_branch_and_the_index_before() {
    let (repo, main, _) = repo_with_feature().await;
    repo.write_file("c.txt", "c\n");
    repo.run(&["add", "c.txt"]).await;
    let staged_tree = repo.run(&["write-tree"]).await;
    let snap = before(&repo).await;
    let head = create_commit(repo.git(), repo.path(), "C", false, &c())
        .await
        .expect("commit");
    let rec = after(&repo, snap, summary("commit")).await.expect("record");
    assert_eq!(rec.refs, [moved(&heads("main"), Some(&main), Some(&head))]);
    assert_eq!(rec.head_before, on("main"));
    assert_eq!(rec.head_after, on("main"));
    assert_eq!(rec.index_before.as_deref(), Some(staged_tree.as_str()));
    assert_eq!(rec.kind, summary("commit"));
}

#[tokio::test]
async fn the_first_commit_creates_the_branch() {
    let repo = TestRepo::new().await;
    repo.write_file("a.txt", "a\n");
    repo.run(&["add", "a.txt"]).await;
    let snap = before(&repo).await;
    let head = create_commit(repo.git(), repo.path(), "A", false, &c())
        .await
        .expect("commit");
    let rec = after(&repo, snap, summary("commit")).await.expect("record");
    assert_eq!(rec.refs, [moved(&heads("main"), None, Some(&head))]);
    assert_eq!(
        rec.head_before,
        on("main"),
        "an unborn branch is still a branch"
    );
}

#[tokio::test]
async fn an_amend_moves_the_branch_to_the_new_commit() {
    let (repo, main, _) = repo_with_feature().await;
    let snap = before(&repo).await;
    let head = create_commit(repo.git(), repo.path(), "B (amended)", true, &c())
        .await
        .expect("amend");
    let rec = after(
        &repo,
        snap,
        OpKind::Amend {
            summary: "B (amended)".into(),
        },
    )
    .await
    .expect("record");
    assert_ne!(head, main);
    assert_eq!(rec.refs, [moved(&heads("main"), Some(&main), Some(&head))]);
}

#[tokio::test]
async fn merges_and_rebases_record_the_branch_that_moved() {
    let (repo, main, feature) = repo_with_feature().await;
    // Fast-forward of main.
    let snap = before(&repo).await;
    repo.run(&["merge", "--ff-only", "--quiet", "feature"])
        .await;
    let kind = OpKind::Merge {
        source: "feature".into(),
        fast_forward: true,
    };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(
        rec.refs,
        [moved(&heads("main"), Some(&main), Some(&feature))]
    );

    // A rebase of another branch onto main: only that branch moves.
    repo.run(&["switch", "--quiet", "-c", "topic", &main]).await;
    let topic = repo.commit_file("t.txt", "t\n", "T").await;
    let snap = before(&repo).await;
    repo.run(&["rebase", "--quiet", "main"]).await;
    let rebased = repo.head().await;
    let kind = OpKind::Rebase {
        onto: "main".into(),
    };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(
        rec.refs,
        [moved(&heads("topic"), Some(&topic), Some(&rebased))]
    );
    assert_eq!(rec.head_after, on("topic"));
}

#[tokio::test]
async fn cherry_pick_revert_and_reset_move_the_current_branch() {
    let (repo, main, feature) = repo_with_feature().await;
    let snap = before(&repo).await;
    let PickOutcome::Done { head: picked, .. } =
        cherry_pick(repo.git(), repo.path(), &feature, false, false, &c())
            .await
            .expect("cherry-pick")
    else {
        panic!("not done");
    };
    let kind = OpKind::CherryPick {
        summary: "F".into(),
    };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(
        rec.refs,
        [moved(&heads("main"), Some(&main), Some(&picked))]
    );

    let snap = before(&repo).await;
    let PickOutcome::Done { head: reverted, .. } =
        revert(repo.git(), repo.path(), &picked, false, false, &c())
            .await
            .expect("revert")
    else {
        panic!("not done");
    };
    let kind = OpKind::Revert {
        summary: "F".into(),
    };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(
        rec.refs,
        [moved(&heads("main"), Some(&picked), Some(&reverted))]
    );

    let snap = before(&repo).await;
    repo.run(&["reset", "--quiet", "--mixed", &main]).await;
    let kind = OpKind::Reset {
        mode: gitcraft_core::ops::reset::ResetMode::Mixed,
        target: main[..7].to_owned(),
    };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(
        rec.refs,
        [moved(&heads("main"), Some(&reverted), Some(&main))]
    );
}

#[tokio::test]
async fn branches_record_creation_renaming_and_deletion() {
    let (repo, main, feature) = repo_with_feature().await;
    let snap = before(&repo).await;
    create_branch(repo.git(), repo.path(), "new", "main", false, &c())
        .await
        .expect("create");
    let kind = OpKind::CreateBranch { name: "new".into() };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(rec.refs, [moved(&heads("new"), None, Some(&main))]);

    let snap = before(&repo).await;
    delete_branch(repo.git(), repo.path(), "feature", &feature, &c())
        .await
        .expect("delete");
    let kind = OpKind::DeleteBranch {
        name: "feature".into(),
    };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(rec.refs, [moved(&heads("feature"), Some(&feature), None)]);

    // Renaming the current branch also moves HEAD.
    let snap = before(&repo).await;
    rename_branch(repo.git(), repo.path(), "main", "trunk", &c())
        .await
        .expect("rename");
    let kind = OpKind::RenameBranch {
        old: "main".into(),
        new: "trunk".into(),
    };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(
        rec.refs,
        [
            moved(&heads("main"), Some(&main), None),
            moved(&heads("trunk"), None, Some(&main)),
        ]
    );
    assert_eq!(rec.head_before, on("main"));
    assert_eq!(rec.head_after, on("trunk"));
}

#[tokio::test]
async fn a_checkout_moves_only_head() {
    let (repo, main, _) = repo_with_feature().await;
    let snap = before(&repo).await;
    let outcome = checkout_branch(repo.git(), repo.path(), "feature", &c())
        .await
        .expect("checkout");
    assert!(matches!(outcome, CheckoutOutcome::Switched));
    let rec = after(&repo, snap, summary("feature"))
        .await
        .expect("record");
    assert!(rec.refs.is_empty());
    assert_eq!(rec.head_before, on("main"));
    assert_eq!(rec.head_after, on("feature"));

    // To a detached commit.
    let snap = before(&repo).await;
    repo.run(&["switch", "--quiet", "--detach", &main]).await;
    let rec = after(&repo, snap, summary("main")).await.expect("record");
    assert_eq!(rec.head_after, HeadAt::Detached { oid: main });
}

#[tokio::test]
async fn tags_record_creation_and_deletion() {
    let (repo, main, _) = repo_with_feature().await;
    let snap = before(&repo).await;
    create_tag(
        repo.git(),
        repo.path(),
        &main,
        "v1",
        Some("Version 1"),
        &c(),
    )
    .await
    .expect("create tag");
    let tag_object = repo.rev_parse("refs/tags/v1").await;
    assert_ne!(
        tag_object, main,
        "an annotated tag points to its tag object"
    );
    let kind = OpKind::CreateTag { name: "v1".into() };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(rec.refs, [moved("refs/tags/v1", None, Some(&tag_object))]);
    assert_eq!(rec.head_before, rec.head_after);

    let snap = before(&repo).await;
    delete_tag(repo.git(), repo.path(), "v1", &tag_object, &c())
        .await
        .expect("delete tag");
    let kind = OpKind::DeleteTag { name: "v1".into() };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(
        rec.refs,
        [moved("refs/tags/v1", Some(&tag_object), None)],
        "the tag object is what comes back, message included"
    );
}

#[tokio::test]
async fn a_pull_records_the_local_branch_but_not_the_remote_one() {
    let (repo, main, _) = repo_with_feature().await;
    let _bare = repo.add_bare_remote("origin").await;
    repo.run(&["branch", "--quiet", "--set-upstream-to=origin/main", "main"])
        .await;
    // Someone else pushes a commit.
    let other = TestRepo::clone_of(_bare.path()).await;
    let theirs = other.commit_file("o.txt", "o\n", "O").await;
    other.run(&["push", "--quiet", "origin", "main"]).await;

    let snap = before(&repo).await;
    let outcome = pull(
        repo.git(),
        repo.path(),
        PullMode::Merge,
        false,
        &c(),
        |_| {},
    )
    .await
    .expect("pull");
    assert!(matches!(outcome, PullOutcome::Pulled { .. }), "{outcome:?}");
    let kind = OpKind::Pull {
        branch: "main".into(),
    };
    let rec = after(&repo, snap, kind).await.expect("record");
    assert_eq!(
        rec.refs,
        [moved(&heads("main"), Some(&main), Some(&theirs))]
    );
}

#[tokio::test]
async fn operations_that_move_no_local_ref_return_nothing() {
    let (repo, _, _) = repo_with_feature().await;
    let bare = repo.add_bare_remote("origin").await;
    let other = TestRepo::clone_of(bare.path()).await;
    other.commit_file("o.txt", "o\n", "O").await;
    other.run(&["push", "--quiet", "origin", "main"]).await;

    // Staging.
    repo.write_file("a.txt", "changed\n");
    let snap = before(&repo).await;
    stage(repo.git(), repo.path(), &["a.txt".to_owned()], &c())
        .await
        .expect("stage");
    assert_eq!(after(&repo, snap, summary("commit")).await, None);

    // Stash (refs/stash is not a branch or a tag).
    let snap = before(&repo).await;
    create_stash(repo.git(), repo.path(), "mine", false, &c())
        .await
        .expect("stash");
    assert_eq!(after(&repo, snap, summary("commit")).await, None);

    // Fetch (only remote-tracking branches move).
    let snap = before(&repo).await;
    fetch(repo.git(), repo.path(), Some("origin"), false, &c(), |_| {})
        .await
        .expect("fetch");
    assert_eq!(after(&repo, snap, summary("commit")).await, None);

    // Push of another branch.
    repo.checkout("feature").await;
    let snap = before(&repo).await;
    push(repo.git(), repo.path(), Some("origin"), None, &c(), |_| {})
        .await
        .expect("push");
    assert_eq!(after(&repo, snap, summary("commit")).await, None);
}

#[tokio::test]
async fn the_fingerprints_say_whether_the_repository_is_still_as_it_was_left() {
    let (repo, _, _) = repo_with_feature().await;
    let snap = before(&repo).await;
    repo.run(&["branch", "other"]).await;
    let rec = after(
        &repo,
        snap,
        OpKind::CreateBranch {
            name: "other".into(),
        },
    )
    .await
    .expect("record");
    let cancel = c();
    let refs = || current_refs(repo.git(), repo.path(), &cancel);
    let full = || current_fingerprint(repo.git(), repo.path(), &cancel);
    assert_eq!(refs().await.expect("refs"), rec.refs_after);
    assert_eq!(full().await.expect("full"), rec.fingerprint_after);

    // An edit changes the full one, not the refs one.
    repo.write_file("a.txt", "edited\n");
    assert_eq!(refs().await.expect("refs"), rec.refs_after);
    assert_ne!(full().await.expect("full"), rec.fingerprint_after);

    // A tag changes both, although HEAD and the files stay.
    repo.run(&["checkout", "--quiet", "--", "a.txt"]).await;
    repo.tag_lightweight("t").await;
    assert_ne!(refs().await.expect("refs"), rec.refs_after);
    assert_ne!(full().await.expect("full"), rec.fingerprint_after);
}

#[tokio::test]
async fn the_snapshot_changes_nothing_and_has_no_index_during_conflicts() {
    let (repo, _, _) = repo_with_feature().await;
    repo.write_file("a.txt", "staged\n");
    repo.run(&["add", "a.txt"]).await;
    repo.write_file("a.txt", "and unstaged\n");
    repo.write_file("new.txt", "untracked\n");
    let state = visible_state(&repo).await;
    let snap = before(&repo).await;
    assert_eq!(visible_state(&repo).await, state);
    let rec = record(
        repo.git(),
        repo.path(),
        snap,
        summary("commit"),
        Saved {
            stash: Some("abc".into()),
            trashed: vec![("x".into(), "/trash/x".into())],
        },
        &c(),
    )
    .await
    .expect("record");
    assert_eq!(rec, None, "nothing moved");

    // A conflicted merge: the index cannot be written as a tree.
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "base\n", "base").await;
    repo.run(&["switch", "--quiet", "-c", "side"]).await;
    repo.commit_file("a.txt", "side\n", "side").await;
    repo.checkout("main").await;
    repo.commit_file("a.txt", "main\n", "main").await;
    let _ = repo
        .write()
        .args(["merge", "--quiet", "side"])
        .accept_exit_code(1)
        .output(&c())
        .await;
    let snap = before(&repo).await;
    repo.run(&["merge", "--abort"]).await;
    repo.run(&["branch", "after-abort"]).await;
    let rec = after(&repo, snap, OpKind::CreateBranch { name: "x".into() })
        .await
        .expect("record");
    assert_eq!(rec.index_before, None);
}

#[tokio::test]
async fn saved_copies_are_kept_in_the_record() {
    let (repo, _, _) = repo_with_feature().await;
    let snap = before(&repo).await;
    repo.run(&["branch", "other"]).await;
    let saved = Saved {
        stash: Some("0123abcd".into()),
        trashed: vec![("notes.txt".into(), "/trash/notes.txt".into())],
    };
    let rec = record(
        repo.git(),
        repo.path(),
        snap,
        OpKind::CreateBranch {
            name: "other".into(),
        },
        saved.clone(),
        &c(),
    )
    .await
    .expect("record")
    .expect("something moved");
    assert_eq!(rec.stash, saved.stash);
    assert_eq!(rec.trashed, saved.trashed);
}
