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
    assert_eq!(check.default_branch, "main", "nothing configured");

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
async fn suggests_the_users_own_branch_never_the_systems() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).expect("folder");
    let write = |name: &str, text: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, text).expect("config");
        path
    };
    // What Git's installer writes on Windows.
    let system = write("system", "[init]\n\tdefaultBranch = master\n");
    let included = write("included", "[init]\n\tdefaultBranch = develop\n");
    let none = write("none", "");
    let own = write("own", "[init]\n\tdefaultBranch = trunk\n");
    let include = write(
        "include",
        &format!(
            "[include]\n\tpath = {}\n",
            included
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        ),
    );
    for (global, expected) in [(none, "main"), (own, "trunk"), (include, "develop")] {
        let git = git()
            .with_env("GIT_CONFIG_NOSYSTEM", "0")
            .with_env("GIT_CONFIG_SYSTEM", &system)
            .with_env("GIT_CONFIG_GLOBAL", &global);
        // The system's value is really there.
        let effective = git
            .read(&repo)
            .args(["config", "--system", "--get", "init.defaultBranch"])
            .output(&c())
            .await
            .expect("config");
        assert_eq!(effective.stdout_lossy().trim(), "master");
        let check = check_init(&git, &repo, &c()).await.expect("check");
        assert_eq!(check.default_branch, expected);
        // And that is the branch the repository gets.
        let target = dir.path().join(expected);
        std::fs::create_dir(&target).expect("folder");
        init_repository(&git, &target, &check.default_branch, &c())
            .await
            .expect("init");
        assert_eq!(head_branch(&target).await, expected);
    }
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
