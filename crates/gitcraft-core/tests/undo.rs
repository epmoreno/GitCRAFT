//! Undo and redo (task 11.2), against real repositories: undoing puts the
//! refs back, and everything else exactly when nothing changed since;
//! redoing takes the refs forward again; uncommitted changes are never
//! lost; and nothing is done when the repository changed, an operation is
//! waiting, or the commits were published since.

use gitcraft_core::CancellationToken;
use gitcraft_core::ops::journal::{OpKind, OpRecord, Saved, record, snapshot};
use gitcraft_core::ops::reset::ResetMode;
use gitcraft_core::ops::undo::{
    Direction, UndoOutcome, UndoPreview, UndoRefusal, UndoRequest, undo, undo_preview,
    undo_stash_message,
};
use gitcraft_core::testing::TestRepo;

fn c() -> CancellationToken {
    CancellationToken::new()
}

/// Refs, HEAD, the index, the status and the contents of every changed or
/// untracked file: two equal values mean the same repository.
async fn full_state(repo: &TestRepo) -> String {
    let refs = repo
        .run(&["for-each-ref", "--format=%(refname) %(objectname)"])
        .await;
    let head = repo
        .write()
        .args(["symbolic-ref", "-q", "HEAD"])
        .accept_exit_code(1)
        .output(&c())
        .await
        .expect("head")
        .stdout_lossy();
    let detached = repo
        .write()
        .args(["rev-parse", "-q", "--verify", "HEAD"])
        .accept_exit_code(1)
        .output(&c())
        .await
        .expect("head oid")
        .stdout_lossy();
    let index = repo.run(&["ls-files", "-s"]).await;
    let status = repo
        .run(&["status", "--porcelain=v2", "--untracked-files=all"])
        .await;
    let diff = repo.run(&["diff", "--binary"]).await;
    let untracked = repo
        .run(&["ls-files", "--others", "--exclude-standard"])
        .await;
    let mut contents = String::new();
    for path in untracked.lines() {
        contents.push_str(&format!(
            "{path}: {}\n",
            std::fs::read_to_string(repo.path().join(path)).unwrap_or_default()
        ));
    }
    format!("{refs}\n{head}\n{detached}\n{index}\n{status}\n{diff}\n{contents}")
}

/// Runs `op` between a snapshot and a record of kind `kind`.
macro_rules! recorded {
    ($repo:expr, $kind:expr, $saved:expr, $op:expr) => {{
        let snap = snapshot($repo.git(), $repo.path(), &c())
            .await
            .expect("snapshot");
        $op;
        record($repo.git(), $repo.path(), snap, $kind, $saved, &c())
            .await
            .expect("record")
            .expect("the operation moved something")
    }};
    ($repo:expr, $kind:expr, $op:expr) => {
        recorded!($repo, $kind, Saved::default(), $op)
    };
}

async fn preview(repo: &TestRepo, rec: &OpRecord, direction: Direction) -> UndoPreview {
    undo_preview(repo.git(), repo.path(), rec, direction, &c())
        .await
        .expect("preview")
}

/// Previews, then does it with the preview's fingerprint.
async fn run(repo: &TestRepo, rec: &OpRecord, direction: Direction, save: bool) -> UndoOutcome {
    let p = preview(repo, rec, direction).await;
    let request = UndoRequest {
        direction,
        expected: p.fingerprint,
        save_changes: save,
    };
    undo(repo.git(), repo.path(), rec, &request, &c())
        .await
        .expect("undo")
}

/// Does it and expects it done; returns the updated record.
async fn done(repo: &TestRepo, rec: &OpRecord, direction: Direction) -> OpRecord {
    match run(repo, rec, direction, false).await {
        UndoOutcome::Done { record, .. } => *record,
        UndoOutcome::NotDone { preview } => panic!("not done: {preview:?}"),
    }
}

fn commit(summary: &str) -> OpKind {
    OpKind::Commit {
        summary: summary.into(),
    }
}

async fn base() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "A").await;
    repo.commit_file("b.txt", "b\n", "B").await;
    repo
}

#[tokio::test]
async fn a_commit_is_undone_exactly_and_redone() {
    let repo = base().await;
    repo.write_file("c.txt", "c\n");
    repo.run(&["add", "c.txt"]).await;
    repo.write_file("a.txt", "unstaged\n");
    let before = full_state(&repo).await;
    let rec = recorded!(
        repo,
        commit("C"),
        repo.run(&["commit", "-q", "-m", "C"]).await
    );
    let after = full_state(&repo).await;

    let p = preview(&repo, &rec, Direction::Undo).await;
    assert_eq!(p.refused, None);
    assert!(p.exact);
    assert_eq!(p.branches.len(), 1);
    assert_eq!(p.branches[0].leaving_total, 1);
    assert_eq!(p.branches[0].leaving[0].subject, "C");
    assert_eq!(p.branches[0].only_in_reflog, 1);

    let undone = done(&repo, &rec, Direction::Undo).await;
    assert_eq!(
        full_state(&repo).await,
        before,
        "its changes are staged again"
    );
    done(&repo, &undone, Direction::Redo).await;
    assert_eq!(full_state(&repo).await, after);
}

#[tokio::test]
async fn several_commits_in_a_row_can_be_undone() {
    let repo = base().await;
    let start = repo.head().await;
    repo.write_file("c.txt", "c\n");
    repo.run(&["add", "c.txt"]).await;
    let first = recorded!(
        repo,
        commit("C"),
        repo.run(&["commit", "-q", "-m", "C"]).await
    );
    repo.write_file("d.txt", "d\n");
    repo.run(&["add", "d.txt"]).await;
    let second = recorded!(
        repo,
        commit("D"),
        repo.run(&["commit", "-q", "-m", "D"]).await
    );

    done(&repo, &second, Direction::Undo).await;
    // D's changes are staged: not exactly as C left it, but its refs are.
    let p = preview(&repo, &first, Direction::Undo).await;
    assert_eq!(p.refused, None);
    assert!(!p.exact);
    done(&repo, &first, Direction::Undo).await;
    assert_eq!(repo.head().await, start);
    let staged = repo.run(&["diff", "--cached", "--name-only"]).await;
    assert_eq!(staged, "c.txt\nd.txt", "both commits' changes stay staged");
}

#[tokio::test]
async fn an_amend_brings_the_original_commit_back() {
    let repo = base().await;
    let original = repo.head().await;
    repo.write_file("b.txt", "b2\n");
    repo.run(&["add", "b.txt"]).await;
    let before = full_state(&repo).await;
    let rec = recorded!(
        repo,
        OpKind::Amend {
            summary: "B2".into()
        },
        repo.run(&["commit", "-q", "--amend", "-m", "B2"]).await
    );
    done(&repo, &rec, Direction::Undo).await;
    assert_eq!(repo.head().await, original);
    assert_eq!(full_state(&repo).await, before);
}

#[tokio::test]
async fn the_first_commit_is_undone_to_an_empty_branch() {
    let repo = TestRepo::new().await;
    repo.write_file("a.txt", "a\n");
    repo.run(&["add", "a.txt"]).await;
    let before = full_state(&repo).await;
    let rec = recorded!(
        repo,
        commit("A"),
        repo.run(&["commit", "-q", "-m", "A"]).await
    );
    let after = full_state(&repo).await;
    let undone = done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);
    done(&repo, &undone, Direction::Redo).await;
    assert_eq!(full_state(&repo).await, after);
}

/// `main` with A, B and C; `feature` from A with F1 and F2.
async fn diverged() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "A").await;
    repo.create_branch("feature").await;
    repo.commit_file("b.txt", "b\n", "B").await;
    repo.commit_file("c.txt", "c\n", "C").await;
    repo.checkout("feature").await;
    repo.commit_file("f1.txt", "f1\n", "F1").await;
    repo.commit_file("f2.txt", "f2\n", "F2").await;
    repo.checkout("main").await;
    repo
}

#[tokio::test]
async fn a_merge_is_undone_exactly_and_redone() {
    let repo = diverged().await;
    repo.write_file("a.txt", "mine\n");
    repo.write_file("notes.txt", "untracked\n");
    let before = full_state(&repo).await;
    let kind = OpKind::Merge {
        source: "feature".into(),
        fast_forward: false,
    };
    let rec = recorded!(repo, kind, repo.merge("feature", "Merge feature").await);
    let p = preview(&repo, &rec, Direction::Undo).await;
    assert!(p.exact);
    assert_eq!(p.branches[0].leaving_total, 3, "the merge and F1, F2");
    assert_eq!(p.branches[0].only_in_reflog, 1, "F1 and F2 stay on feature");

    let undone = done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);

    done(&repo, &undone, Direction::Redo).await;
    assert_eq!(repo.head().await, rec.refs[0].after.clone().expect("after"));
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).expect("a.txt"),
        "mine\n"
    );
}

#[tokio::test]
async fn changes_made_after_the_operation_are_kept() {
    let repo = diverged().await;
    let start = repo.head().await;
    let kind = OpKind::Merge {
        source: "feature".into(),
        fast_forward: false,
    };
    let rec = recorded!(repo, kind, repo.merge("feature", "Merge feature").await);
    repo.write_file("a.txt", "edited later\n");
    repo.write_file("b.txt", "staged later\n");
    repo.run(&["add", "b.txt"]).await;

    let p = preview(&repo, &rec, Direction::Undo).await;
    assert_eq!(p.refused, None);
    assert!(!p.exact);
    assert!(
        p.blocking.is_empty(),
        "a.txt and b.txt are not in the merge"
    );
    let outcome = run(&repo, &rec, Direction::Undo, false).await;
    assert!(
        matches!(
            outcome,
            UndoOutcome::Done {
                exact: false,
                unstaged: false,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(repo.head().await, start);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).expect("a.txt"),
        "edited later\n"
    );
    assert_eq!(
        repo.run(&["diff", "--cached", "--name-only"]).await,
        "b.txt"
    );
}

#[tokio::test]
async fn changes_in_the_way_go_to_the_stash_only_if_accepted() {
    let repo = diverged().await;
    let kind = OpKind::Merge {
        source: "feature".into(),
        fast_forward: false,
    };
    let rec = recorded!(repo, kind, repo.merge("feature", "Merge feature").await);
    repo.write_file("f1.txt", "mine\n");
    let state = full_state(&repo).await;

    let p = preview(&repo, &rec, Direction::Undo).await;
    assert_eq!(p.blocking.len(), 1);
    assert_eq!(p.blocking[0].path, "f1.txt");
    let outcome = run(&repo, &rec, Direction::Undo, false).await;
    assert!(matches!(outcome, UndoOutcome::NotDone { .. }));
    assert_eq!(full_state(&repo).await, state, "nothing was touched");

    let UndoOutcome::Done { saved, .. } = run(&repo, &rec, Direction::Undo, true).await else {
        panic!("not done");
    };
    let saved = saved.expect("a stash entry");
    let listed = repo.run(&["stash", "list", "--format=%H %gs"]).await;
    assert!(listed.contains(&saved));
    assert!(listed.contains(&undo_stash_message(Direction::Undo)));
    assert_eq!(
        repo.run(&["show", &format!("{saved}:f1.txt")]).await,
        "mine"
    );
}

#[tokio::test]
async fn a_rebase_and_a_fast_forward_are_undone_and_redone() {
    let repo = diverged().await;
    repo.checkout("feature").await;
    let before = full_state(&repo).await;
    let rec = recorded!(
        repo,
        OpKind::Rebase {
            onto: "main".into()
        },
        repo.run(&["rebase", "--quiet", "main"]).await
    );
    let after = full_state(&repo).await;
    let undone = done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);
    done(&repo, &undone, Direction::Redo).await;
    assert_eq!(full_state(&repo).await, after);

    // Fast-forward of another branch: only its ref moves.
    repo.checkout("main").await;
    repo.run(&["switch", "--quiet", "-c", "ahead"]).await;
    repo.commit_file("x.txt", "x\n", "X").await;
    repo.checkout("main").await;
    repo.create_branch("behind").await;
    let before = full_state(&repo).await;
    let kind = OpKind::Merge {
        source: "ahead".into(),
        fast_forward: true,
    };
    let rec = recorded!(
        repo,
        kind,
        repo.run(&["fetch", "--quiet", ".", "ahead:behind"]).await
    );
    let undone = done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);
    done(&repo, &undone, Direction::Redo).await;
}

#[tokio::test]
async fn resets_are_undone_exactly_with_their_copies() {
    let repo = base().await;
    let target = repo.rev_parse("HEAD~1").await;
    repo.write_file("b.txt", "changed\n");
    repo.run(&["add", "b.txt"]).await;
    repo.write_file("a.txt", "unstaged\n");
    let before = full_state(&repo).await;

    // Mixed: the index comes back.
    let kind = OpKind::Reset {
        mode: ResetMode::Mixed,
        target: target.clone(),
    };
    let rec = recorded!(
        repo,
        kind,
        repo.run(&["reset", "-q", "--mixed", &target]).await
    );
    done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);

    // Hard, with the changes saved first (as the reset does).
    let stash = repo.run(&["stash", "create"]).await;
    repo.run(&["stash", "store", "-m", "copy", &stash]).await;
    let before = full_state(&repo).await;
    let kind = OpKind::Reset {
        mode: ResetMode::Hard,
        target: target.clone(),
    };
    let saved = Saved {
        stash: Some(stash.clone()),
        trashed: Vec::new(),
    };
    let rec = recorded!(
        repo,
        kind,
        saved,
        repo.run(&["reset", "-q", "--hard", &target]).await
    );
    let p = preview(&repo, &rec, Direction::Undo).await;
    assert_eq!(p.restores_stash.as_deref(), Some(stash.as_str()));
    assert_eq!(p.branches[0].arriving_total, 1);
    let undone = done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);
    assert_eq!(undone.stash, None, "the copy was given back");
}

#[tokio::test]
async fn a_cherry_pick_and_a_revert_are_undone() {
    let repo = diverged().await;
    let f1 = repo.rev_parse("feature~1").await;
    let before = full_state(&repo).await;
    let kind = OpKind::CherryPick {
        summary: "F1".into(),
    };
    let rec = recorded!(repo, kind, repo.run(&["cherry-pick", &f1]).await);
    done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);

    let kind = OpKind::Revert {
        summary: "C".into(),
    };
    let rec = recorded!(repo, kind, repo.run(&["revert", "--no-edit", "HEAD"]).await);
    done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);
}

#[tokio::test]
async fn a_checkout_goes_back_carrying_the_changes() {
    let repo = diverged().await;
    repo.write_file("a.txt", "mine\n");
    let rec = recorded!(
        repo,
        OpKind::Checkout {
            branch: "feature".into()
        },
        repo.checkout("feature").await
    );
    let undone = done(&repo, &rec, Direction::Undo).await;
    assert_eq!(repo.run(&["symbolic-ref", "HEAD"]).await, "refs/heads/main");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("a.txt")).expect("a.txt"),
        "mine\n"
    );
    done(&repo, &undone, Direction::Redo).await;
    assert_eq!(
        repo.run(&["symbolic-ref", "HEAD"]).await,
        "refs/heads/feature"
    );
}

#[tokio::test]
async fn branches_and_tags_come_back() {
    let repo = diverged().await;
    let before = full_state(&repo).await;

    let rec = recorded!(
        repo,
        OpKind::DeleteBranch {
            name: "feature".into()
        },
        repo.run(&["branch", "-D", "feature"]).await
    );
    let p = preview(&repo, &rec, Direction::Undo).await;
    assert_eq!(p.branches[0].arriving_total, 2, "F1 and F2 come back");
    done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);

    // Renaming the current branch.
    let rec = recorded!(
        repo,
        OpKind::RenameBranch {
            old: "main".into(),
            new: "trunk".into()
        },
        repo.run(&["branch", "-m", "main", "trunk"]).await
    );
    let undone = done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);
    done(&repo, &undone, Direction::Redo).await;
    assert_eq!(
        repo.run(&["symbolic-ref", "HEAD"]).await,
        "refs/heads/trunk"
    );
    repo.run(&["branch", "-m", "trunk", "main"]).await;

    // An annotated tag, message included.
    repo.tag_annotated("v1", "Version one").await;
    let before = full_state(&repo).await;
    let rec = recorded!(
        repo,
        OpKind::DeleteTag { name: "v1".into() },
        repo.run(&["tag", "-d", "v1"]).await
    );
    assert_eq!(preview(&repo, &rec, Direction::Undo).await.tags.len(), 1);
    done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);
    assert_eq!(
        repo.run(&["tag", "-l", "--format=%(contents:subject)", "v1"])
            .await,
        "Version one"
    );

    let rec = recorded!(
        repo,
        OpKind::CreateBranch { name: "new".into() },
        repo.run(&["branch", "new"]).await
    );
    let undone = done(&repo, &rec, Direction::Undo).await;
    assert_eq!(full_state(&repo).await, before);
    done(&repo, &undone, Direction::Redo).await;
    assert!(repo.run(&["branch", "--list", "new"]).await.contains("new"));
}

#[tokio::test]
async fn a_detached_commit_is_undone() {
    let repo = base().await;
    let start = repo.head().await;
    repo.run(&["switch", "--quiet", "--detach"]).await;
    repo.write_file("d.txt", "d\n");
    repo.run(&["add", "d.txt"]).await;
    let before = full_state(&repo).await;
    let rec = recorded!(
        repo,
        commit("D"),
        repo.run(&["commit", "-q", "-m", "D"]).await
    );
    done(&repo, &rec, Direction::Undo).await;
    assert_eq!(repo.head().await, start);
    assert_eq!(full_state(&repo).await, before);
}

#[tokio::test]
async fn nothing_is_done_when_the_refs_changed_or_an_operation_waits() {
    let repo = diverged().await;
    let rec = recorded!(
        repo,
        commit("D"),
        repo.commit_file("d.txt", "d\n", "D").await
    );

    // A new commit since: the branch is not where the operation left it.
    repo.commit_file("e.txt", "e\n", "E").await;
    let state = full_state(&repo).await;
    let p = preview(&repo, &rec, Direction::Undo).await;
    assert_eq!(p.refused, Some(UndoRefusal::Changed));
    assert!(matches!(
        run(&repo, &rec, Direction::Undo, true).await,
        UndoOutcome::NotDone { .. }
    ));
    assert_eq!(full_state(&repo).await, state);

    // Nor while a merge waits.
    let rec = recorded!(
        repo,
        OpKind::CreateBranch { name: "x".into() },
        repo.run(&["branch", "x"]).await
    );
    repo.write_file("a.txt", "main side\n");
    repo.run(&["commit", "-qam", "main side"]).await;
    repo.checkout("feature").await;
    repo.write_file("a.txt", "feature side\n");
    repo.run(&["commit", "-qam", "feature side"]).await;
    repo.checkout("main").await;
    let _ = repo
        .write()
        .args(["merge", "--quiet", "feature"])
        .accept_exit_code(1)
        .output(&c())
        .await;
    let p = preview(&repo, &rec, Direction::Undo).await;
    assert_eq!(p.refused, Some(UndoRefusal::InProgress));
    repo.run(&["merge", "--abort"]).await;

    // A preview that is no longer current.
    let rec = recorded!(
        repo,
        OpKind::CreateBranch { name: "y".into() },
        repo.run(&["branch", "y"]).await
    );
    let p = preview(&repo, &rec, Direction::Undo).await;
    repo.write_file("later.txt", "later\n");
    let request = UndoRequest {
        direction: Direction::Undo,
        expected: p.fingerprint,
        save_changes: false,
    };
    let outcome = undo(repo.git(), repo.path(), &rec, &request, &c())
        .await
        .expect("undo");
    assert!(matches!(outcome, UndoOutcome::NotDone { .. }));
    assert!(repo.run(&["branch", "--list", "y"]).await.contains('y'));
}

#[tokio::test]
async fn published_commits_are_not_taken_out_but_pulled_ones_are() {
    let repo = base().await;
    let bare = repo.add_bare_remote("origin").await;
    repo.run(&["branch", "-q", "--set-upstream-to=origin/main", "main"])
        .await;

    // A commit, then pushed: undoing it would need a forced push.
    let rec = recorded!(
        repo,
        commit("C"),
        repo.commit_file("c.txt", "c\n", "C").await
    );
    assert_eq!(preview(&repo, &rec, Direction::Undo).await.refused, None);
    repo.run(&["push", "--quiet", "origin", "main"]).await;
    let p = preview(&repo, &rec, Direction::Undo).await;
    let Some(UndoRefusal::Published {
        branch,
        commits,
        remotes,
    }) = p.refused
    else {
        panic!("not refused: {p:?}");
    };
    assert_eq!(branch, "main");
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].subject, "C");
    assert_eq!(remotes, ["origin/main"]);

    // A pull of someone else's commit: they were already on the remote.
    let other = TestRepo::clone_of(bare.path()).await;
    other.commit_file("o.txt", "o\n", "O").await;
    other.run(&["push", "--quiet", "origin", "main"]).await;
    let before = full_state(&repo).await;
    let rec = recorded!(
        repo,
        OpKind::Pull {
            branch: "main".into()
        },
        repo.run(&["pull", "--quiet", "--ff-only"]).await
    );
    let p = preview(&repo, &rec, Direction::Undo).await;
    assert_eq!(p.refused, None);
    done(&repo, &rec, Direction::Undo).await;
    // The remote branch keeps what was fetched; the local one goes back.
    let local = full_state(&repo).await;
    assert_ne!(local, before, "origin/main moved with the fetch");
    assert_eq!(
        repo.head().await,
        rec.refs[0].before.clone().expect("before")
    );
}

#[tokio::test]
async fn a_branch_open_in_another_worktree_is_not_moved() {
    let repo = diverged().await;
    let rec = recorded!(
        repo,
        OpKind::Merge {
            source: "main".into(),
            fast_forward: true
        },
        repo.run(&["fetch", "--quiet", ".", "main:feature", "--force"])
            .await
    );
    let elsewhere = tempfile::tempdir().expect("temp dir");
    let path = elsewhere.path().join("wt");
    repo.run(&[
        "worktree",
        "add",
        "--quiet",
        path.to_str().expect("utf-8"),
        "feature",
    ])
    .await;
    let p = preview(&repo, &rec, Direction::Undo).await;
    assert_eq!(
        p.refused,
        Some(UndoRefusal::CheckedOutElsewhere {
            branch: "feature".into()
        })
    );
}

#[tokio::test]
async fn redo_is_refused_once_something_changed_after_the_undo() {
    let repo = base().await;
    let rec = recorded!(
        repo,
        commit("C"),
        repo.commit_file("c.txt", "c\n", "C").await
    );
    let undone = done(&repo, &rec, Direction::Undo).await;
    assert_eq!(preview(&repo, &undone, Direction::Redo).await.refused, None);
    repo.run(&["commit", "-q", "--allow-empty", "-m", "another C"])
        .await;
    let state = full_state(&repo).await;
    let p = preview(&repo, &undone, Direction::Redo).await;
    assert_eq!(p.refused, Some(UndoRefusal::Changed));
    assert!(matches!(
        run(&repo, &undone, Direction::Redo, true).await,
        UndoOutcome::NotDone { .. }
    ));
    assert_eq!(full_state(&repo).await, state);
}
