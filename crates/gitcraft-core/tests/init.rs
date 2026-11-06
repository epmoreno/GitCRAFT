//! `git init` in a folder chosen by the user.

use gitcraft_core::ops::init::{check_init, init_repository};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind, Git};

fn git() -> Git {
    gitcraft_core::testing::isolated_git()
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

async fn head_branch(path: &std::path::Path) -> String {
    let out = git()
        .read(path)
        .args(["symbolic-ref", "--short", "HEAD"])
        .output(&c())
        .await
        .expect("symbolic-ref");
    out.stdout_lossy().trim().to_owned()
}

#[tokio::test]
async fn creates_a_repository_with_the_chosen_first_branch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let check = check_init(&git(), dir.path(), &c()).await.expect("check");
    assert!(!check.already_repository);
    assert_eq!(check.inside, None);
    assert!(check.empty);
    // The user's init.defaultBranch, or "main".
    let configured = git()
        .read(dir.path())
        .args(["config", "--get", "init.defaultBranch"])
        .accept_exit_code(1)
        .output(&c())
        .await
        .expect("config")
        .stdout_lossy();
    let expected = configured.trim();
    assert_eq!(
        check.default_branch,
        if expected.is_empty() {
            "main"
        } else {
            expected
        }
    );

    init_repository(&git(), dir.path(), "trunk", &c())
        .await
        .expect("init");
    assert_eq!(head_branch(dir.path()).await, "trunk");

    let check = check_init(&git(), dir.path(), &c()).await.expect("check");
    assert!(check.already_repository);
    let err = init_repository(&git(), dir.path(), "main", &c())
        .await
        .expect_err("already a repository");
    assert_eq!(err.kind(), ErrorKind::AlreadyARepository);
    assert_eq!(head_branch(dir.path()).await, "trunk", "untouched");
}

#[tokio::test]
async fn keeps_existing_files_untracked() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("notas.txt"), "hola\n").expect("write");
    let check = check_init(&git(), dir.path(), &c()).await.expect("check");
    assert!(!check.empty);
    init_repository(&git(), dir.path(), "main", &c())
        .await
        .expect("init");
    let status = git()
        .read(dir.path())
        .args(["status", "--porcelain"])
        .output(&c())
        .await
        .expect("status")
        .stdout_lossy();
    assert_eq!(status.trim(), "?? notas.txt");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notas.txt")).expect("read"),
        "hola\n"
    );
}

#[tokio::test]
async fn warns_about_nesting_and_refuses_bad_input() {
    let outer = TestRepo::new().await;
    let sub = outer.path().join("sub");
    std::fs::create_dir(&sub).expect("mkdir");
    let check = check_init(&git(), &sub, &c()).await.expect("check");
    assert!(!check.already_repository);
    let inside = check.inside.expect("inside the outer repository");
    assert_eq!(
        std::fs::canonicalize(inside).expect("canonical"),
        std::fs::canonicalize(outer.path()).expect("canonical")
    );

    let dir = tempfile::tempdir().expect("tempdir");
    for bad in ["a..b", "-x", "con espacio", ""] {
        let err = init_repository(&git(), dir.path(), bad, &c())
            .await
            .expect_err("invalid branch");
        assert_eq!(err.kind(), ErrorKind::InvalidBranchName, "{bad:?}");
    }
    assert!(!dir.path().join(".git").exists(), "nothing created");

    let err = check_init(&git(), &dir.path().join("missing"), &c())
        .await
        .expect_err("missing");
    assert_eq!(err.kind(), ErrorKind::PathNotFound);
}
