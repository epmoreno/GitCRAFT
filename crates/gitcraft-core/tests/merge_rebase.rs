//! Merging or rebasing one branch with another (task 8), against real
//! repositories: the previews never change a ref, the index or the working
//! tree, and undo puts everything back exactly.

use gitcraft_core::ops::branch::BlockingFile;
use gitcraft_core::ops::merge_rebase::{
    MergeRequest, MoveBlock, MoveCase, MoveOutcome, MovePreview, MoveRecord, MoveRefusal,
    merge as merge_branch, merge_preview, merge_stash_message, rebase_preview, undo_move,
};
use gitcraft_core::ops::remote::abort_in_progress;
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, GitError};

fn c() -> CancellationToken {
    CancellationToken::new()
}

fn heads(name: &str) -> String {
    format!("refs/heads/{name}")
}

async fn merge(repo: &TestRepo, source: &str, target: &str) -> MovePreview {
    merge_preview(repo.git(), repo.path(), source, target, &c())
        .await
        .expect("merge preview")
}

async fn rebase(repo: &TestRepo, source: &str, target: &str) -> MovePreview {
    rebase_preview(repo.git(), repo.path(), source, target, &c())
        .await
        .expect("rebase preview")
}

fn subjects(p: &MovePreview) -> Vec<&str> {
    p.commits
        .iter()
        .map(|c| c.commit.subject.as_str())
        .collect()
}

fn tracked(path: &str) -> BlockingFile {
    BlockingFile {
        path: path.to_owned(),
        untracked: false,
    }
}

/// Refs, index and working tree, to check a preview changed nothing.
async fn snapshot(repo: &TestRepo) -> String {
    let refs = repo
        .run(&["for-each-ref", "--format=%(refname) %(objectname)"])
        .await;
    let status = repo
        .run(&["status", "--porcelain=v2", "--untracked-files=all"])
        .await;
    let index = repo.run(&["ls-files", "-s"]).await;
    format!("{refs}\n{status}\n{index}")
}

/// main: base → "main edits other". feature (from base): "feature edits
/// f" → "feature adds new.txt". Current branch: main.
async fn diverged() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.write_file("f.txt", "base\n");
    repo.write_file("other.txt", "other\n");
    repo.commit_all("base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    repo.commit_file("f.txt", "feature\n", "feature edits f")
        .await;
    repo.commit_file("new.txt", "new\n", "feature adds new.txt")
        .await;
    repo.checkout("main").await;
    repo.commit_file("other.txt", "other (main)\n", "main edits other")
        .await;
    repo
}

/// main: base. ahead (from base): "a1" → "a2". Current branch: main.
async fn behind() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.create_branch("ahead").await;
    repo.checkout("ahead").await;
    repo.commit_file("f.txt", "a1\n", "a1").await;
    repo.commit_file("g.txt", "a2\n", "a2").await;
    repo.checkout("main").await;
    repo
}

#[tokio::test]
async fn nothing_to_do_when_the_branch_already_has_everything() {
    let repo = behind().await;
    // Merging main (behind) into ahead: ahead already has it.
    let p = merge(&repo, &heads("main"), &heads("ahead")).await;
    assert_eq!((p.case, p.refused), (MoveCase::UpToDate, None));
    // Rebasing ahead onto main: ahead already sits on main.
    repo.checkout("ahead").await;
    let p = rebase(&repo, &heads("ahead"), &heads("main")).await;
    assert_eq!((p.case, p.refused), (MoveCase::UpToDate, None));
    assert!(p.commits.is_empty());
}

#[tokio::test]
async fn a_fast_forward_of_the_current_branch() {
    let repo = behind().await;
    let before = snapshot(&repo).await;
    let p = merge(&repo, &heads("ahead"), &heads("main")).await;
    assert_eq!(
        (p.case, p.refused, p.blocked),
        (MoveCase::FastForward, None, None)
    );
    assert_eq!(p.moving, "main");
    assert_eq!(p.commits_total, 2);
    assert_eq!(subjects(&p), ["a2", "a1"]);
    assert_eq!(p.message.as_deref(), Some("Merge branch 'ahead'"));
    assert!(p.conflicts.is_empty());
    // Rebasing main onto ahead is the same fast-forward.
    let p = rebase(&repo, &heads("main"), &heads("ahead")).await;
    assert_eq!((p.case, p.refused), (MoveCase::FastForward, None));
    assert_eq!(subjects(&p), ["a2", "a1"]);
    assert_eq!(snapshot(&repo).await, before);
}

#[tokio::test]
async fn a_fast_forward_of_another_local_branch_is_allowed() {
    let repo = behind().await;
    repo.checkout("ahead").await;
    // main is not current, but a fast-forward only moves its ref.
    let p = merge(&repo, &heads("ahead"), &heads("main")).await;
    assert_eq!(p.current.as_deref(), Some("ahead"));
    assert_eq!((p.case, p.refused), (MoveCase::FastForward, None));
    assert_eq!(p.moving, "main");
    // The working tree does not take part: nothing is in the way.
    repo.write_file("f.txt", "local change\n");
    let p = merge(&repo, &heads("ahead"), &heads("main")).await;
    assert_eq!((p.refused, p.blocked), (None, None));
}

#[tokio::test]
async fn a_fast_forward_of_a_branch_checked_out_elsewhere_is_refused() {
    let repo = behind().await;
    repo.checkout("ahead").await;
    let other = tempfile::tempdir().expect("temp dir");
    let place = other.path().join("wt");
    repo.run(&[
        "worktree",
        "add",
        "--quiet",
        place.to_str().expect("utf-8"),
        "main",
    ])
    .await;
    let p = merge(&repo, &heads("ahead"), &heads("main")).await;
    assert_eq!(p.refused, Some(MoveRefusal::CheckedOutElsewhere));
}

#[tokio::test]
async fn a_clean_merge_lists_the_commits_and_git_s_message() {
    let repo = diverged().await;
    let before = snapshot(&repo).await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(
        (p.case, p.refused, p.blocked),
        (MoveCase::Merge, None, None)
    );
    assert_eq!(
        (p.moving.as_str(), p.current.as_deref()),
        ("main", Some("main"))
    );
    assert_eq!(p.source.name, "feature");
    assert_eq!(p.target.name, "main");
    assert_eq!(p.commits_total, 2);
    assert_eq!(subjects(&p), ["feature adds new.txt", "feature edits f"]);
    assert!(p.conflicts.is_empty() && p.exact);
    assert_eq!(p.message.as_deref(), Some("Merge branch 'feature'"));
    assert_eq!(snapshot(&repo).await, before);
}

#[tokio::test]
async fn the_message_names_the_target_unless_it_is_main() {
    let repo = diverged().await;
    repo.run(&["switch", "--quiet", "-c", "dev"]).await;
    let p = merge(&repo, &heads("feature"), &heads("dev")).await;
    assert_eq!(
        p.message.as_deref(),
        Some("Merge branch 'feature' into dev")
    );
}

#[tokio::test]
async fn a_remote_branch_can_be_the_source_but_never_moves() {
    let repo = diverged().await;
    let _remote = repo.add_bare_remote("origin").await;
    let p = merge(&repo, "refs/remotes/origin/feature", &heads("main")).await;
    assert_eq!((p.case, p.refused), (MoveCase::Merge, None));
    assert!(p.source.remote);
    assert_eq!(p.source.name, "origin/feature");
    assert_eq!(
        p.message.as_deref(),
        Some("Merge remote-tracking branch 'origin/feature'")
    );
    // Rebasing main onto origin/feature: main moves, which is fine.
    let p = rebase(&repo, &heads("main"), "refs/remotes/origin/feature").await;
    assert_eq!((p.case, p.refused), (MoveCase::Rebase, None));
    // Merging into origin/main or rebasing origin/main: it would move.
    let p = merge(&repo, &heads("feature"), "refs/remotes/origin/main").await;
    assert_eq!(p.refused, Some(MoveRefusal::RemoteMoves));
    let p = rebase(&repo, "refs/remotes/origin/main", &heads("feature")).await;
    assert_eq!(p.refused, Some(MoveRefusal::RemoteMoves));
}

#[tokio::test]
async fn a_merge_conflict_is_predicted_exactly() {
    let repo = diverged().await;
    repo.commit_file("f.txt", "main\n", "main edits f").await;
    let before = snapshot(&repo).await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.case, MoveCase::Merge);
    assert_eq!(p.conflicts, ["f.txt"]);
    assert!(p.exact);
    assert_eq!(snapshot(&repo).await, before);

    // git's own merge agrees.
    let _ = repo.write().args(["merge", "feature"]).output(&c()).await;
    assert_eq!(
        repo.run(&["diff", "--name-only", "--diff-filter=U"]).await,
        "f.txt"
    );
}

#[tokio::test]
async fn a_clean_rebase_lists_what_is_replayed_and_what_is_skipped() {
    let repo = diverged().await;
    // main already has the change of "feature adds new.txt".
    repo.run(&["cherry-pick", "feature"]).await;
    repo.checkout("feature").await;
    let before = snapshot(&repo).await;
    let p = rebase(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(
        (p.case, p.refused, p.blocked),
        (MoveCase::Rebase, None, None)
    );
    assert_eq!(p.moving, "feature");
    assert_eq!(subjects(&p), ["feature edits f"]);
    assert_eq!((p.commits_total, p.skipped), (1, 1));
    assert!(p.conflicts.is_empty() && p.stops_at.is_none() && p.exact);
    assert_eq!(p.message, None);
    assert!(p.published_in.is_empty());
    assert_eq!(snapshot(&repo).await, before);
}

#[tokio::test]
async fn a_rebase_says_where_it_would_stop() {
    let repo = diverged().await;
    repo.commit_file("new.txt", "main's own\n", "main adds new.txt")
        .await;
    repo.checkout("feature").await;
    repo.commit_file("h.txt", "h\n", "feature adds h").await;
    let before = snapshot(&repo).await;
    let p = rebase(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.case, MoveCase::Rebase);
    assert_eq!(p.commits_total, 3);
    assert_eq!(
        subjects(&p),
        ["feature edits f", "feature adds new.txt", "feature adds h"]
    );
    let stop = p.stops_at.expect("it stops");
    assert_eq!(stop.index, 2);
    assert_eq!(stop.commit.subject, "feature adds new.txt");
    assert_eq!(p.conflicts, ["new.txt"]);
    assert!(p.exact);
    assert_eq!(snapshot(&repo).await, before);

    // git's own rebase stops exactly there.
    let _ = repo.write().args(["rebase", "main"]).output(&c()).await;
    assert_eq!(repo.rev_parse("REBASE_HEAD").await, stop.commit.oid);
    assert_eq!(
        repo.run(&["diff", "--name-only", "--diff-filter=U"]).await,
        "new.txt"
    );
}

#[tokio::test]
async fn a_rebase_marks_the_published_commits() {
    let repo = diverged().await;
    repo.checkout("feature").await;
    let _remote = repo.add_bare_remote("origin").await;
    repo.commit_file("h.txt", "h\n", "feature adds h").await;
    let p = rebase(&repo, &heads("feature"), &heads("main")).await;
    let published: Vec<(&str, bool)> = p
        .commits
        .iter()
        .map(|c| (c.commit.subject.as_str(), c.published))
        .collect();
    assert_eq!(
        published,
        [
            ("feature edits f", true),
            ("feature adds new.txt", true),
            ("feature adds h", false),
        ]
    );
    assert_eq!(p.published_in, ["origin/feature"]);
}

#[tokio::test]
async fn a_rebase_with_merges_in_the_range_is_refused() {
    let repo = diverged().await;
    repo.checkout("feature").await;
    repo.create_branch("side").await;
    repo.checkout("side").await;
    repo.commit_file("side.txt", "side\n", "side").await;
    repo.checkout("feature").await;
    repo.commit_file("h.txt", "h\n", "feature adds h").await;
    repo.merge("side", "Merge side").await;
    let p = rebase(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.refused, Some(MoveRefusal::MergesInRange));
}

#[tokio::test]
async fn only_the_current_branch_moves() {
    let repo = diverged().await;
    // Merging into feature (not current, and not a fast-forward).
    let p = merge(&repo, &heads("main"), &heads("feature")).await;
    assert_eq!(p.refused, Some(MoveRefusal::NotCurrent));
    assert_eq!(p.moving, "feature");
    // Rebasing feature while on main.
    let p = rebase(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.refused, Some(MoveRefusal::NotCurrent));
    // The same branch on both sides.
    let p = merge(&repo, &heads("main"), &heads("main")).await;
    assert_eq!(p.refused, Some(MoveRefusal::SameBranch));
}

#[tokio::test]
async fn refused_with_a_detached_head_or_an_operation_waiting() {
    let repo = diverged().await;
    repo.commit_file("f.txt", "main\n", "main edits f").await;
    repo.run(&["switch", "--quiet", "--detach", "HEAD"]).await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.refused, Some(MoveRefusal::Detached));

    repo.checkout("main").await;
    let _ = repo.write().args(["merge", "feature"]).output(&c()).await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.refused, Some(MoveRefusal::InProgress));
}

#[tokio::test]
async fn unrelated_histories_are_refused() {
    let repo = diverged().await;
    repo.run(&["switch", "--quiet", "--orphan", "lonely"]).await;
    repo.commit_file("x.txt", "x\n", "lonely").await;
    let p = merge(&repo, &heads("main"), &heads("lonely")).await;
    assert_eq!(p.refused, Some(MoveRefusal::Unrelated));
}

#[tokio::test]
async fn a_merge_commit_needs_nothing_staged() {
    let repo = diverged().await;
    // A staged change in a file the merge does not touch still blocks it.
    repo.write_file("other.txt", "staged\n");
    repo.run(&["add", "other.txt"]).await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.blocked, Some(MoveBlock::StagedChanges));
    assert_eq!(p.blocking, [tracked("other.txt")]);
}

#[tokio::test]
async fn a_merge_is_blocked_by_changes_in_the_files_it_changes() {
    let repo = diverged().await;
    repo.write_file("other.txt", "unstaged, not touched by the merge\n");
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.blocked, None);
    repo.write_file("f.txt", "unstaged\n");
    repo.write_file("new.txt", "untracked\n");
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.blocked, Some(MoveBlock::Overwrite));
    assert_eq!(
        p.blocking,
        [
            tracked("f.txt"),
            BlockingFile {
                path: "new.txt".into(),
                untracked: true
            }
        ]
    );
}

#[tokio::test]
async fn a_fast_forward_is_blocked_only_by_the_files_it_changes() {
    let repo = behind().await;
    // Staged, in a file the fast-forward does not change: fine (git agrees).
    repo.write_file("other.txt", "x\n");
    repo.run(&["add", "other.txt"]).await;
    let p = merge(&repo, &heads("ahead"), &heads("main")).await;
    assert_eq!(p.blocked, None);
    repo.write_file("g.txt", "untracked\n");
    let p = merge(&repo, &heads("ahead"), &heads("main")).await;
    assert_eq!(p.blocked, Some(MoveBlock::Overwrite));
    assert_eq!(
        p.blocking,
        [BlockingFile {
            path: "g.txt".into(),
            untracked: true
        }]
    );
}

#[tokio::test]
async fn a_rebase_needs_a_clean_working_tree() {
    let repo = diverged().await;
    repo.checkout("feature").await;
    repo.write_file("other.txt", "anything\n");
    let p = rebase(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.blocked, Some(MoveBlock::RebaseNeedsClean));
    assert_eq!(p.blocking, [tracked("other.txt")]);
}

#[tokio::test]
async fn the_fingerprint_follows_both_branches() {
    let repo = diverged().await;
    let first = merge(&repo, &heads("feature"), &heads("main")).await;
    let again = merge(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(first.fingerprint, again.fingerprint);
    repo.checkout("feature").await;
    repo.commit_file("h.txt", "h\n", "feature adds h").await;
    repo.checkout("main").await;
    let moved = merge(&repo, &heads("feature"), &heads("main")).await;
    assert_ne!(first.fingerprint, moved.fingerprint);
}

#[tokio::test]
async fn only_existing_branches_are_accepted() {
    let repo = diverged().await;
    for bad in [
        "main",
        "refs/tags/v1",
        "refs/heads/-x",
        "refs/remotes/origin/HEAD",
    ] {
        let err = merge_preview(repo.git(), repo.path(), bad, &heads("main"), &c())
            .await
            .expect_err(bad);
        assert!(
            matches!(err, GitError::InvalidBranchName(_)),
            "{bad}: {err:?}"
        );
    }
    let err = merge_preview(
        repo.git(),
        repo.path(),
        &heads("gone"),
        &heads("main"),
        &c(),
    )
    .await
    .expect_err("missing");
    assert!(matches!(err, GitError::BranchChanged(_)), "{err:?}");
}

// ---- 8.2: merging, and undoing it ----

/// Everything a merge and its undo may touch: refs, index, status, the
/// stash and every file's content.
async fn full_state(repo: &TestRepo) -> String {
    let mut text = snapshot(repo).await;
    text.push_str(&repo.run(&["stash", "list", "--format=%H %gs"]).await);
    text.push_str(
        &repo
            .run(&["rev-parse", "--symbolic-full-name", "HEAD"])
            .await,
    );
    let mut files: Vec<_> = walk(repo.path());
    files.sort();
    for path in files {
        let rel = path.strip_prefix(repo.path()).expect("inside");
        text.push_str(&format!(
            "\n{}: {}",
            rel.display(),
            std::fs::read_to_string(&path).unwrap_or_default()
        ));
    }
    text
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read dir").flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

fn request(p: &MovePreview) -> MergeRequest {
    MergeRequest {
        source: p.source.refname.clone(),
        target: p.target.refname.clone(),
        expected: p.fingerprint.clone(),
        no_ff: false,
        message: None,
        save_changes: false,
        allow_conflicts: false,
    }
}

async fn run_merge(repo: &TestRepo, req: &MergeRequest) -> (MoveOutcome, Option<MoveRecord>) {
    merge_branch(repo.git(), repo.path(), req, &c())
        .await
        .expect("merge")
}

async fn undo(repo: &TestRepo, record: &MoveRecord) -> Result<(), GitError> {
    undo_move(repo.git(), repo.path(), record, &c()).await
}

#[tokio::test]
async fn a_merge_commit_with_an_edited_message_then_undo() {
    let repo = diverged().await;
    // Local work the merge does not touch: kept, and kept by the undo.
    repo.write_file("other.txt", "unstaged, untouched\n");
    repo.write_file("notes.txt", "untracked\n");
    let before = full_state(&repo).await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    let mut req = request(&p);
    req.message = Some("Bring feature in\n\nWith a body.".into());
    let (outcome, record) = run_merge(&repo, &req).await;
    let MoveOutcome::Done {
        head,
        fast_forward,
        saved,
    } = outcome
    else {
        panic!("expected a merge: {outcome:?}");
    };
    assert_eq!((fast_forward, saved), (false, None));
    assert_eq!(head, repo.head().await);
    assert_eq!(
        repo.run(&["log", "-1", "--format=%B"]).await,
        "Bring feature in\n\nWith a body."
    );
    assert_eq!(
        repo.run(&["rev-list", "--parents", "-1", "HEAD"])
            .await
            .split(' ')
            .count(),
        3
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("other.txt")).expect("read"),
        "unstaged, untouched\n"
    );

    undo(&repo, &record.expect("record")).await.expect("undo");
    assert_eq!(full_state(&repo).await, before);
}

#[tokio::test]
async fn a_blank_message_means_git_s_own() {
    let repo = diverged().await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    let mut req = request(&p);
    req.message = Some("  \n".into());
    run_merge(&repo, &req).await;
    assert_eq!(
        repo.run(&["log", "-1", "--format=%B"]).await,
        "Merge branch 'feature'"
    );
}

#[tokio::test]
async fn a_fast_forward_of_the_current_branch_then_undo() {
    let repo = behind().await;
    // Staged and unstaged work in files the fast-forward does not change.
    repo.write_file("other.txt", "staged\n");
    repo.run(&["add", "other.txt"]).await;
    repo.write_file("other.txt", "staged, then changed\n");
    let before = full_state(&repo).await;
    let p = merge(&repo, &heads("ahead"), &heads("main")).await;
    let (outcome, record) = run_merge(&repo, &request(&p)).await;
    assert!(
        matches!(
            outcome,
            MoveOutcome::Done {
                fast_forward: true,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(repo.head().await, repo.rev_parse("ahead").await);
    undo(&repo, &record.expect("record")).await.expect("undo");
    assert_eq!(full_state(&repo).await, before);
}

#[tokio::test]
async fn always_a_merge_commit_when_asked() {
    let repo = behind().await;
    let p = merge(&repo, &heads("ahead"), &heads("main")).await;
    let before = full_state(&repo).await;
    let mut req = request(&p);
    req.no_ff = true;
    let (outcome, record) = run_merge(&repo, &req).await;
    assert!(
        matches!(
            outcome,
            MoveOutcome::Done {
                fast_forward: false,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(
        repo.run(&["log", "-1", "--format=%s"]).await,
        "Merge branch 'ahead'"
    );
    undo(&repo, &record.expect("record")).await.expect("undo");
    assert_eq!(full_state(&repo).await, before);
}

#[tokio::test]
async fn a_fast_forward_of_another_branch_then_undo() {
    let repo = behind().await;
    repo.checkout("ahead").await;
    repo.write_file("f.txt", "local change\n");
    let before = full_state(&repo).await;
    let p = merge(&repo, &heads("ahead"), &heads("main")).await;
    let (outcome, record) = run_merge(&repo, &request(&p)).await;
    assert!(
        matches!(
            outcome,
            MoveOutcome::Done {
                fast_forward: true,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(repo.rev_parse("main").await, repo.rev_parse("ahead").await);
    // Only main's ref moved.
    assert_eq!(
        std::fs::read_to_string(repo.path().join("f.txt")).expect("read"),
        "local change\n"
    );
    let record = record.expect("record");
    assert!(!record.current);
    undo(&repo, &record).await.expect("undo");
    assert_eq!(full_state(&repo).await, before);
}

#[tokio::test]
async fn undo_refuses_once_something_changed() {
    let repo = diverged().await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    let (_, record) = run_merge(&repo, &request(&p)).await;
    let record = record.expect("record");
    repo.write_file("f.txt", "edited after the merge\n");
    let after = full_state(&repo).await;
    let err = undo(&repo, &record).await.expect_err("refused");
    assert!(matches!(err, GitError::UndoConflict(_)), "{err:?}");
    assert_eq!(full_state(&repo).await, after);

    // Another branch's fast-forward, then that branch moved again.
    let repo = behind().await;
    repo.checkout("ahead").await;
    let p = merge(&repo, &heads("ahead"), &heads("main")).await;
    let (_, record) = run_merge(&repo, &request(&p)).await;
    repo.checkout("main").await;
    let err = undo(&repo, &record.expect("record"))
        .await
        .expect_err("refused");
    assert!(matches!(err, GitError::UndoConflict(_)), "{err:?}");
}

#[tokio::test]
async fn conflicts_only_when_accepted_and_the_message_is_kept() {
    let repo = diverged().await;
    repo.commit_file("f.txt", "main\n", "main edits f").await;
    let before = full_state(&repo).await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    let mut req = request(&p);
    req.message = Some("My merge".into());
    let (outcome, record) = run_merge(&repo, &req).await;
    assert!(matches!(
        outcome,
        MoveOutcome::NotDone { changed: false, .. }
    ));
    assert!(record.is_none());
    assert_eq!(full_state(&repo).await, before);

    req.allow_conflicts = true;
    let (outcome, record) = run_merge(&repo, &req).await;
    let MoveOutcome::Conflicted { files, saved } = outcome else {
        panic!("expected conflicts: {outcome:?}");
    };
    assert_eq!(
        (files, saved, record),
        (vec!["f.txt".to_owned()], None, None)
    );
    // Task 9's continue will commit with the user's message.
    let merge_msg = std::fs::read_to_string(repo.path().join(".git/MERGE_MSG")).expect("MERGE_MSG");
    assert!(merge_msg.starts_with("My merge"), "{merge_msg}");
    abort_in_progress(repo.git(), repo.path(), &c())
        .await
        .expect("abort");
    assert_eq!(full_state(&repo).await, before);
}

#[tokio::test]
async fn changes_in_the_way_are_saved_only_when_asked_and_undo_brings_them_back() {
    let repo = diverged().await;
    repo.write_file("other.txt", "staged\n");
    repo.run(&["add", "other.txt"]).await;
    let before = snapshot(&repo).await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    assert_eq!(p.blocked, Some(MoveBlock::StagedChanges));
    let mut req = request(&p);
    let (outcome, _) = run_merge(&repo, &req).await;
    assert!(matches!(outcome, MoveOutcome::NotDone { .. }));
    assert_eq!(snapshot(&repo).await, before);

    req.save_changes = true;
    let (outcome, record) = run_merge(&repo, &req).await;
    let MoveOutcome::Done { saved, .. } = outcome else {
        panic!("expected a merge: {outcome:?}");
    };
    let saved = saved.expect("saved");
    assert_eq!(
        repo.run(&["stash", "list", "--format=%H %gs"]).await,
        format!("{saved} On main: {}", merge_stash_message("feature"))
    );
    // Undo puts the branch back and the saved changes too, staged as they
    // were; the stash entry stays.
    undo(&repo, &record.expect("record")).await.expect("undo");
    let without_stash_ref = |text: String| {
        text.lines()
            .filter(|l| !l.starts_with("refs/stash "))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(without_stash_ref(snapshot(&repo).await), before);
    assert_eq!(repo.run(&["stash", "list", "--format=%H"]).await, saved);
}

#[tokio::test]
async fn nothing_is_done_if_the_repository_changed_since_the_preview() {
    let repo = diverged().await;
    let p = merge(&repo, &heads("feature"), &heads("main")).await;
    repo.write_file("other.txt", "changed after the preview\n");
    let before = full_state(&repo).await;
    let (outcome, record) = run_merge(&repo, &request(&p)).await;
    let MoveOutcome::NotDone {
        changed, preview, ..
    } = outcome
    else {
        panic!("expected nothing: {outcome:?}");
    };
    assert!(changed);
    assert_ne!(preview.fingerprint, p.fingerprint);
    assert!(record.is_none());
    assert_eq!(full_state(&repo).await, before);
}
