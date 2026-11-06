//! `open_repository` against real repositories.

use std::path::Path;

use gitcraft_core::error::ErrorKind;
use gitcraft_core::ops::repo::open_repository;
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, Git, GitError};

fn git() -> Git {
    gitcraft_core::testing::isolated_git()
}

/// git reports canonical paths (e.g. /private/var on macOS for /var).
fn canonical(path: &Path) -> String {
    path.canonicalize()
        .expect("canonicalize")
        .to_string_lossy()
        .into_owned()
}

#[tokio::test]
async fn opens_a_repository_root() {
    let repo = TestRepo::new().await;
    let info = open_repository(&git(), repo.path(), &CancellationToken::new())
        .await
        .expect("open");
    assert_eq!(info.path, canonical(repo.path()));
    assert_eq!(info.git_dir, canonical(&repo.path().join(".git")));
    assert_eq!(info.common_dir, info.git_dir);
    assert_eq!(
        info.name,
        repo.path().file_name().expect("name").to_string_lossy()
    );
}

#[tokio::test]
async fn opening_a_subdirectory_opens_the_enclosing_repository() {
    let repo = TestRepo::new().await;
    repo.write_file("src/nested/file.txt", "x\n");
    let info = open_repository(
        &git(),
        &repo.path().join("src/nested"),
        &CancellationToken::new(),
    )
    .await
    .expect("open");
    assert_eq!(info.path, canonical(repo.path()));
}

#[tokio::test]
async fn a_linked_worktree_shares_the_main_common_dir() {
    let (repo, _) = TestRepo::sample_history().await;
    let parent = tempfile::tempdir().expect("tempdir");
    let worktree = parent.path().join("wt");
    repo.run(&[
        "worktree",
        "add",
        "--quiet",
        worktree.to_str().expect("utf-8"),
        "topic",
    ])
    .await;
    let info = open_repository(&git(), &worktree, &CancellationToken::new())
        .await
        .expect("open worktree");
    assert_eq!(info.path, canonical(&worktree));
    assert_ne!(info.git_dir, info.common_dir);
    assert_eq!(info.common_dir, canonical(&repo.path().join(".git")));
}

#[tokio::test]
async fn opens_a_repository_with_history() {
    let (repo, _) = TestRepo::sample_history().await;
    let info = open_repository(&git(), repo.path(), &CancellationToken::new())
        .await
        .expect("open");
    assert_eq!(info.path, canonical(repo.path()));
}

#[tokio::test]
async fn a_plain_folder_is_not_a_repository() {
    let dir = tempfile::tempdir().expect("tempdir");
    // GIT_CEILING_DIRECTORIES is not needed: temp dirs are never inside a repo.
    let err = open_repository(&git(), dir.path(), &CancellationToken::new())
        .await
        .expect_err("not a repo");
    assert_eq!(err.kind(), ErrorKind::NotARepository, "{err}");
    let details = err.details().expect("details");
    assert!(details.starts_with("$ git rev-parse --show-toplevel --absolute-git-dir"));
    assert!(details.contains("not a git repository"), "{details}");
}

#[tokio::test]
async fn a_missing_path_is_reported_before_running_git() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("gone");
    let err = open_repository(&git(), &missing, &CancellationToken::new())
        .await
        .expect_err("missing");
    assert!(
        matches!(&err, GitError::PathNotFound(p) if *p == missing),
        "{err}"
    );
    assert_eq!(err.kind(), ErrorKind::PathNotFound);
}

#[tokio::test]
async fn a_file_is_not_a_directory() {
    let repo = TestRepo::new().await;
    let file = repo.write_file("file.txt", "x\n");
    let err = open_repository(&git(), &file, &CancellationToken::new())
        .await
        .expect_err("file");
    assert_eq!(err.kind(), ErrorKind::PathNotFound);
}

#[tokio::test]
async fn bare_repositories_are_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    git()
        .write(dir.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["init", "--bare", "--quiet"])
        .output(&CancellationToken::new())
        .await
        .expect("init --bare");
    let err = open_repository(&git(), dir.path(), &CancellationToken::new())
        .await
        .expect_err("bare");
    assert_eq!(err.kind(), ErrorKind::BareRepository, "{err}");
}
