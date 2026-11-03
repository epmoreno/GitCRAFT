//! Staging and unstaging files against real repositories.

use gitcraft_core::ops::stage::{stage, stage_all, unstage, unstage_all};
use gitcraft_core::ops::status::working_status;
use gitcraft_core::parse::diff_tree::FileStatus;
use gitcraft_core::parse::status::WorkingStatus;
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, Git};

fn git() -> Git {
    Git::discover().expect("git must be installed to run the tests")
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

fn p(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|s| s.to_string()).collect()
}

async fn status(repo: &TestRepo) -> WorkingStatus {
    working_status(&git(), repo.path(), &c())
        .await
        .expect("status")
}

fn names(entries: &[gitcraft_core::parse::status::StatusEntry]) -> Vec<(&str, FileStatus)> {
    entries
        .iter()
        .map(|e| (e.path.as_str(), e.status))
        .collect()
}

async fn repo_with_changes() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.write_file("a.txt", "a\n");
    repo.write_file("gone.txt", "bye\n");
    repo.write_file("old.txt", "same\ncontent\nhere\n");
    repo.commit_all("base").await;
    repo.write_file("a.txt", "changed\n");
    std::fs::remove_file(repo.path().join("gone.txt")).expect("rm");
    repo.write_file("dir/new.txt", "new\n");
    repo
}

#[tokio::test]
async fn stages_modified_deleted_and_untracked_files() {
    let repo = repo_with_changes().await;
    stage(
        &git(),
        repo.path(),
        &p(&["a.txt", "gone.txt", "dir/"]),
        &c(),
    )
    .await
    .expect("stage");
    let s = status(&repo).await;
    assert_eq!(
        names(&s.staged),
        [
            ("a.txt", FileStatus::Modified),
            ("dir/new.txt", FileStatus::Added),
            ("gone.txt", FileStatus::Deleted),
        ]
    );
    assert!(s.unstaged.is_empty() && s.untracked.is_empty());
}

#[tokio::test]
async fn unstaging_keeps_the_working_tree() {
    let repo = repo_with_changes().await;
    stage_all(&git(), repo.path(), &c())
        .await
        .expect("stage all");
    unstage(&git(), repo.path(), &p(&["a.txt", "dir/new.txt"]), &c())
        .await
        .expect("unstage");
    let s = status(&repo).await;
    assert_eq!(names(&s.staged), [("gone.txt", FileStatus::Deleted)]);
    assert_eq!(names(&s.unstaged), [("a.txt", FileStatus::Modified)]);
    assert_eq!(names(&s.untracked), [("dir/", FileStatus::Untracked)]);
    // The file contents were not touched.
    let a = std::fs::read_to_string(repo.path().join("a.txt")).expect("read");
    assert_eq!(a, "changed\n");
    assert!(repo.path().join("dir/new.txt").exists());
}

#[tokio::test]
async fn unstages_a_rename_with_both_paths() {
    let repo = repo_with_changes().await;
    repo.run(&["mv", "old.txt", "renamed.txt"]).await;
    assert_eq!(status(&repo).await.staged[0].status, FileStatus::Renamed);
    unstage(&git(), repo.path(), &p(&["renamed.txt", "old.txt"]), &c())
        .await
        .expect("unstage");
    let s = status(&repo).await;
    assert!(s.staged.is_empty(), "{:?}", s.staged);
    assert!(
        s.unstaged
            .iter()
            .any(|e| e.path == "old.txt" && e.status == FileStatus::Deleted)
    );
    assert!(s.untracked.iter().any(|e| e.path == "renamed.txt"));
}

#[tokio::test]
async fn works_before_the_first_commit() {
    let repo = TestRepo::new().await;
    repo.write_file("first.txt", "x\n");
    repo.write_file("second.txt", "y\n");
    stage_all(&git(), repo.path(), &c()).await.expect("stage");
    assert_eq!(status(&repo).await.staged.len(), 2);
    unstage(&git(), repo.path(), &p(&["first.txt"]), &c())
        .await
        .expect("unstage");
    let s = status(&repo).await;
    assert_eq!(names(&s.staged), [("second.txt", FileStatus::Added)]);
    unstage_all(&git(), repo.path(), &c())
        .await
        .expect("unstage all");
    let s = status(&repo).await;
    assert!(s.staged.is_empty());
    assert_eq!(s.untracked.len(), 2);
}

#[tokio::test]
async fn unstage_all_keeps_an_in_progress_merge() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.create_branch("other").await;
    repo.commit_file("g.txt", "ours\n", "ours").await;
    repo.checkout("other").await;
    repo.commit_file("h.txt", "theirs\n", "theirs").await;
    repo.checkout("main").await;
    repo.run(&["merge", "--no-commit", "--no-ff", "--quiet", "other"])
        .await;
    assert!(repo.path().join(".git/MERGE_HEAD").exists());
    unstage_all(&git(), repo.path(), &c())
        .await
        .expect("unstage all");
    assert!(repo.path().join(".git/MERGE_HEAD").exists(), "merge kept");
}

#[tokio::test]
async fn paths_are_literal() {
    let repo = TestRepo::new().await;
    repo.commit_file("keep.txt", "k\n", "base").await;
    repo.write_file("a.txt", "a\n");
    repo.write_file("*.txt", "star\n");
    repo.write_file("-n", "dash\n");
    stage(&git(), repo.path(), &p(&["*.txt", "-n"]), &c())
        .await
        .expect("stage");
    let s = status(&repo).await;
    assert_eq!(
        names(&s.staged),
        [("*.txt", FileStatus::Added), ("-n", FileStatus::Added)]
    );
    assert_eq!(names(&s.untracked), [("a.txt", FileStatus::Untracked)]);
}

/// B.4: the UI stages a folder by listing each of its files.
#[tokio::test]
async fn stages_and_unstages_a_whole_folder_by_its_files() {
    let repo = TestRepo::new().await;
    repo.commit_file("keep.txt", "k\n", "base").await;
    repo.write_file("-dir/a b.txt", "a\n");
    repo.write_file("-dir/sub/c.txt", "c\n");
    repo.write_file("-dir2/d.txt", "d\n");
    let folder = p(&["-dir/a b.txt", "-dir/sub/c.txt"]);
    stage(&git(), repo.path(), &folder, &c())
        .await
        .expect("stage folder");
    let s = status(&repo).await;
    assert_eq!(
        names(&s.staged),
        [
            ("-dir/a b.txt", FileStatus::Added),
            ("-dir/sub/c.txt", FileStatus::Added)
        ]
    );
    // The sibling folder whose name starts the same is left alone.
    assert_eq!(names(&s.untracked), [("-dir2/", FileStatus::Untracked)]);

    unstage(&git(), repo.path(), &folder, &c())
        .await
        .expect("unstage folder");
    assert!(status(&repo).await.staged.is_empty());
}

#[tokio::test]
async fn rejects_empty_paths_and_ignores_empty_lists() {
    let repo = repo_with_changes().await;
    assert!(stage(&git(), repo.path(), &p(&[""]), &c()).await.is_err());
    stage(&git(), repo.path(), &[], &c()).await.expect("no-op");
    unstage(&git(), repo.path(), &[], &c())
        .await
        .expect("no-op");
    assert!(status(&repo).await.staged.is_empty());
}
