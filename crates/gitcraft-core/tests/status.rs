//! Working directory status against real repositories.

use std::path::PathBuf;

use gitcraft_core::ops::status::working_status;
use gitcraft_core::parse::diff_tree::FileStatus;
use gitcraft_core::parse::status::{ConflictKind, WorkingStatus, parse_status};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, Git};

fn git() -> Git {
    gitcraft_core::testing::isolated_git()
}

async fn status(repo: &TestRepo) -> WorkingStatus {
    working_status(&git(), repo.path(), &CancellationToken::new())
        .await
        .expect("status")
}

fn paths(entries: &[gitcraft_core::parse::status::StatusEntry]) -> Vec<(&str, FileStatus)> {
    entries
        .iter()
        .map(|e| (e.path.as_str(), e.status))
        .collect()
}

/// Staged, unstaged, both, added, deleted, renamed and untracked changes.
async fn busy_repo() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.write_file("staged.txt", "a\n");
    repo.write_file("unstaged.txt", "a\n");
    repo.write_file("both.txt", "a\n");
    repo.write_file("gone.txt", "a\n");
    repo.write_file("old name.txt", "same content\nacross the rename\n");
    repo.commit_all("base").await;

    repo.write_file("staged.txt", "b\n");
    repo.write_file("both.txt", "b\n");
    repo.write_file("new.txt", "new\n");
    repo.run(&["add", "staged.txt", "both.txt", "new.txt"])
        .await;
    repo.run(&["mv", "old name.txt", "new name.txt"]).await;
    repo.write_file("both.txt", "c\n");
    repo.write_file("unstaged.txt", "b\n");
    std::fs::remove_file(repo.path().join("gone.txt")).expect("rm");
    repo.write_file("notes/todo.md", "x\n");
    repo
}

#[tokio::test]
async fn reports_every_kind_of_change() {
    let repo = busy_repo().await;
    let s = status(&repo).await;
    assert_eq!(s.branch.name.as_deref(), Some("main"));
    assert!(s.branch.oid.is_some());

    assert_eq!(
        paths(&s.staged),
        [
            ("both.txt", FileStatus::Modified),
            ("new name.txt", FileStatus::Renamed),
            ("new.txt", FileStatus::Added),
            ("staged.txt", FileStatus::Modified),
        ]
    );
    assert_eq!(s.staged[1].old_path.as_deref(), Some("old name.txt"));
    assert_eq!(
        paths(&s.unstaged),
        [
            ("both.txt", FileStatus::Modified),
            ("gone.txt", FileStatus::Deleted),
            ("unstaged.txt", FileStatus::Modified),
        ]
    );
    assert_eq!(paths(&s.untracked), [("notes/", FileStatus::Untracked)]);
    assert!(s.conflicted.is_empty());
}

#[tokio::test]
async fn reports_merge_conflicts() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.create_branch("other").await;
    repo.commit_file("f.txt", "ours\n", "ours").await;
    repo.checkout("other").await;
    repo.commit_file("f.txt", "theirs\n", "theirs").await;
    repo.checkout("main").await;
    let merge = repo
        .write()
        .args(["merge", "--quiet", "other"])
        .accept_exit_code(1)
        .output(&CancellationToken::new())
        .await
        .expect("merge");
    assert_eq!(merge.exit_code, Some(1), "the merge conflicts");

    let s = status(&repo).await;
    assert_eq!(s.conflicted.len(), 1);
    assert_eq!(s.conflicted[0].path, "f.txt");
    assert_eq!(s.conflicted[0].kind, ConflictKind::BothModified);
}

#[tokio::test]
async fn clean_and_unborn_repositories() {
    let (repo, _) = TestRepo::sample_history().await;
    assert!(status(&repo).await.is_clean());

    let empty = TestRepo::new().await;
    empty.write_file("first.txt", "x\n");
    let s = status(&empty).await;
    assert_eq!(s.branch.oid, None);
    assert_eq!(paths(&s.untracked), [("first.txt", FileStatus::Untracked)]);
}

#[tokio::test]
async fn reading_status_does_not_touch_the_index() {
    let repo = busy_repo().await;
    let index = repo.path().join(".git/index");
    let before = std::fs::metadata(&index)
        .expect("index")
        .modified()
        .expect("mtime");
    // Make every stat entry stale so a normal `git status` would rewrite it.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    repo.write_file("staged.txt", "b\n");
    status(&repo).await;
    let after = std::fs::metadata(&index)
        .expect("index")
        .modified()
        .expect("mtime");
    assert_eq!(before, after);
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/status_v2.z")
}

/// Real porcelain v2 output; regenerate with
/// `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test status`.
#[tokio::test]
async fn status_fixture_matches_current_git_output() {
    let repo = busy_repo().await;
    let fresh = repo
        .read()
        .args([
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=normal",
            "--find-renames",
        ])
        .output(&CancellationToken::new())
        .await
        .expect("status")
        .stdout;
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(fixture(), &fresh).expect("write fixture");
    }
    assert!(std::fs::read(fixture()).expect("fixture") == fresh);
}

#[test]
fn parses_the_status_fixture() {
    let s = parse_status(&std::fs::read(fixture()).expect("fixture")).expect("parse");
    assert_eq!(s.staged.len(), 4);
    assert_eq!(s.unstaged.len(), 3);
    assert_eq!(s.untracked.len(), 1);
}
