//! Continuing, skipping and aborting stopped merges, rebases, cherry-picks
//! and reverts from the app.

use std::collections::BTreeMap;
use std::path::Path;

use gitcraft_core::ops::in_progress::{
    ContinueOutcome, continue_in_progress, prepared_message, skip_current,
};
use gitcraft_core::ops::remote::abort_in_progress;
use gitcraft_core::ops::status::in_progress;
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind};

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

async fn resolve(repo: &TestRepo, path: &str, contents: &str) {
    repo.write_file(path, contents);
    repo.run(&["add", "--", path]).await;
}

async fn cont(repo: &TestRepo, message: Option<&str>) -> ContinueOutcome {
    continue_in_progress(repo.git(), repo.path(), message, &c())
        .await
        .expect("continue")
}

async fn skip(repo: &TestRepo) -> ContinueOutcome {
    skip_current(repo.git(), repo.path(), &c())
        .await
        .expect("skip")
}

async fn subjects(repo: &TestRepo, range: &str) -> Vec<String> {
    repo.run(&["log", "--format=%s", range])
        .await
        .lines()
        .map(str::to_owned)
        .collect()
}

async fn stopped(repo: &TestRepo) -> bool {
    in_progress(repo.git(), repo.path(), &c())
        .await
        .expect("in_progress")
        .is_some()
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
async fn merge_continues_with_the_prepared_message() {
    let repo = diverged().await;
    let main = repo.head().await;
    let feature = repo.rev_parse("feature").await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;

    // Nothing happens while a file is still in conflict.
    assert_eq!(
        cont(&repo, None).await,
        ContinueOutcome::ConflictsLeft {
            conflicts: vec!["f.txt".into()]
        }
    );
    assert_eq!(repo.head().await, main);

    // git's message, without its "# Conflicts:" comment lines.
    let message = prepared_message(repo.git(), repo.path(), &c())
        .await
        .expect("message");
    assert_eq!(message.as_deref(), Some("Merge branch 'feature'\n"));

    resolve(&repo, "f.txt", "both\n").await;
    assert_eq!(cont(&repo, None).await, ContinueOutcome::Done);
    assert!(!stopped(&repo).await);
    assert_eq!(
        repo.run(&["log", "-1", "--format=%P"]).await,
        format!("{main} {feature}")
    );
    assert_eq!(
        repo.run(&["log", "-1", "--format=%B"]).await,
        "Merge branch 'feature'"
    );
    assert_eq!(repo.run(&["show", "HEAD:f.txt"]).await, "both");
}

#[tokio::test]
async fn merge_continues_with_the_users_message() {
    let repo = diverged().await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;
    resolve(&repo, "f.txt", "both\n").await;
    assert_eq!(
        cont(&repo, Some("Join feature\n\nKept both lines.\n")).await,
        ContinueOutcome::Done
    );
    assert_eq!(
        repo.run(&["log", "-1", "--format=%B"]).await,
        "Join feature\n\nKept both lines."
    );
}

#[tokio::test]
async fn a_failing_hook_leaves_the_merge_as_it_was() {
    let repo = diverged().await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;
    resolve(&repo, "f.txt", "both\n").await;
    let hooks = repo.path().join(".git/hooks");
    std::fs::create_dir_all(&hooks).expect("hooks dir");
    let hook = hooks.join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho 'hook says no' >&2\nexit 1\n").expect("hook");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let main = repo.head().await;
    let index = repo.run(&["ls-files", "-s"]).await;

    let err = continue_in_progress(repo.git(), repo.path(), None, &c())
        .await
        .expect_err("the hook refuses");
    assert!(
        err.details().unwrap_or_default().contains("hook says no"),
        "{err:?}"
    );
    assert!(stopped(&repo).await);
    assert_eq!(repo.head().await, main);
    assert_eq!(repo.run(&["ls-files", "-s"]).await, index);
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

#[tokio::test]
async fn rebase_continues_through_two_conflicts() {
    let repo = rebase_repo().await;
    stop_on_conflict(&repo, &["rebase", "main"]).await;
    resolve(&repo, "a.txt", "main + 1\n").await;
    assert_eq!(
        cont(&repo, None).await,
        ContinueOutcome::StoppedAgain {
            conflicts: vec!["a.txt".into()]
        }
    );
    resolve(&repo, "a.txt", "main + 3\n").await;
    assert_eq!(cont(&repo, None).await, ContinueOutcome::Done);

    assert!(!stopped(&repo).await);
    assert_eq!(repo.run(&["branch", "--show-current"]).await, "topic");
    assert_eq!(
        subjects(&repo, "main..topic").await,
        vec!["topic three", "topic two", "topic one"]
    );
    // Linear: every replayed commit has one parent, the first one main.
    assert_eq!(
        repo.run(&["rev-list", "--min-parents=2", "main..topic"])
            .await,
        ""
    );
    assert_eq!(repo.run(&["show", "topic:a.txt"]).await, "main + 3");
    assert_eq!(repo.run(&["show", "topic~2:a.txt"]).await, "main + 1");
}

#[tokio::test]
async fn an_empty_rebase_commit_is_reported_and_skipped() {
    let repo = rebase_repo().await;
    stop_on_conflict(&repo, &["rebase", "main"]).await;
    // Resolving by keeping main's version leaves "topic one" empty.
    resolve(&repo, "a.txt", "main\n").await;
    assert_eq!(cont(&repo, None).await, ContinueOutcome::NothingToCommit);
    assert!(stopped(&repo).await, "nothing was done");

    assert_eq!(
        skip(&repo).await,
        ContinueOutcome::StoppedAgain {
            conflicts: vec!["a.txt".into()]
        }
    );
    resolve(&repo, "a.txt", "main + 3\n").await;
    assert_eq!(cont(&repo, None).await, ContinueOutcome::Done);
    assert_eq!(
        subjects(&repo, "main..topic").await,
        vec!["topic three", "topic two"]
    );
}

#[tokio::test]
async fn an_edit_stop_is_not_an_empty_commit() {
    let repo = rebase_repo().await;
    let out = repo
        .write()
        .env("GIT_SEQUENCE_EDITOR", "sed -i.bak -e '1s/^pick/edit/'")
        .args(["rebase", "-i", "HEAD~2"])
        .output(&c())
        .await
        .expect("rebase -i");
    assert_eq!(out.exit_code, Some(0));
    assert!(stopped(&repo).await);
    assert_eq!(cont(&repo, None).await, ContinueOutcome::Done);
    assert_eq!(
        subjects(&repo, "-3").await,
        vec!["topic three", "topic two", "topic one"]
    );
}

#[tokio::test]
async fn cherry_pick_continue_and_skip() {
    let repo = diverged().await;
    let feature = repo.rev_parse("feature").await;
    stop_on_conflict(&repo, &["cherry-pick", &feature]).await;
    resolve(&repo, "f.txt", "both\n").await;
    assert_eq!(cont(&repo, None).await, ContinueOutcome::Done);
    assert_eq!(subjects(&repo, "-1").await, vec!["feature change"]);

    // Picking it again with the same resolution changes nothing.
    let head = repo.head().await;
    stop_on_conflict(&repo, &["cherry-pick", &feature]).await;
    resolve(&repo, "f.txt", "both\n").await;
    assert_eq!(cont(&repo, None).await, ContinueOutcome::NothingToCommit);
    assert_eq!(skip(&repo).await, ContinueOutcome::Done);
    assert!(!stopped(&repo).await);
    assert_eq!(repo.head().await, head);
}

#[tokio::test]
async fn revert_continues() {
    let repo = diverged().await;
    let base = repo.rev_parse("main~1").await;
    stop_on_conflict(&repo, &["revert", "--no-edit", &base]).await;
    resolve(&repo, "f.txt", "reverted\n").await;
    assert_eq!(cont(&repo, None).await, ContinueOutcome::Done);
    assert_eq!(subjects(&repo, "-1").await, vec!["Revert \"base\""]);
}

#[tokio::test]
async fn skipping_a_merge_is_refused() {
    let repo = diverged().await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;
    let err = skip_current(repo.git(), repo.path(), &c())
        .await
        .expect_err("merge");
    assert_eq!(err.kind(), ErrorKind::Unknown);
    assert!(stopped(&repo).await);
}

// --- Aborting puts everything back ---------------------------------------

/// HEAD, every ref, the index and every file of the working tree.
async fn snapshot(repo: &TestRepo) -> (String, String, String, BTreeMap<String, Vec<u8>>) {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("list") {
            let path = entry.expect("entry").path();
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("inside")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.insert(rel, std::fs::read(&path).expect("read"));
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(repo.path(), repo.path(), &mut files);
    (
        repo.run(&["symbolic-ref", "-q", "HEAD"]).await,
        repo.run(&["for-each-ref", "--format=%(refname) %(objectname)"])
            .await,
        repo.run(&["ls-files", "-s"]).await,
        files,
    )
}

#[tokio::test]
async fn aborting_a_merge_after_resolving_part_puts_everything_back() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "base\n", "base").await;
    repo.commit_file("b.txt", "base\n", "base b").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    repo.write_file("a.txt", "feature\n");
    repo.write_file("b.txt", "feature\n");
    repo.commit_all("feature").await;
    repo.checkout("main").await;
    repo.write_file("a.txt", "main\n");
    repo.write_file("b.txt", "main\n");
    repo.commit_all("main").await;
    repo.write_file("notes.txt", "untracked, kept\n");
    let before = snapshot(&repo).await;

    stop_on_conflict(&repo, &["merge", "feature"]).await;
    resolve(&repo, "a.txt", "resolved\n").await;
    abort_in_progress(repo.git(), repo.path(), &c())
        .await
        .expect("abort");
    assert!(!stopped(&repo).await);
    assert_eq!(snapshot(&repo).await, before);
}

#[tokio::test]
async fn aborting_a_rebase_after_continuing_once_puts_everything_back() {
    let repo = rebase_repo().await;
    let before = snapshot(&repo).await;
    stop_on_conflict(&repo, &["rebase", "main"]).await;
    resolve(&repo, "a.txt", "main + 1\n").await;
    assert!(matches!(
        cont(&repo, None).await,
        ContinueOutcome::StoppedAgain { .. }
    ));
    resolve(&repo, "a.txt", "half done\n").await;
    abort_in_progress(repo.git(), repo.path(), &c())
        .await
        .expect("abort");
    assert!(!stopped(&repo).await);
    assert_eq!(snapshot(&repo).await, before);
}
