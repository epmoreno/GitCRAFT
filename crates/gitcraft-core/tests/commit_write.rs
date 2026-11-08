//! Creating and amending commits against real repositories.

use gitcraft_core::ops::commit::commit_details;
use gitcraft_core::ops::commit_write::{amend_info, compose_message, create_commit};
use gitcraft_core::ops::status::working_status;
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind, Git};

fn git() -> Git {
    gitcraft_core::testing::isolated_git()
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

#[tokio::test]
async fn commits_the_staged_changes_with_summary_and_description() {
    let (repo, ids) = TestRepo::sample_history().await;
    repo.write_file("staged.txt", "s\n");
    repo.write_file("unstaged.txt", "u\n");
    repo.run(&["add", "staged.txt"]).await;

    let message = compose_message(
        "Añade el archivo",
        "Explica \"por qué\" | con $(símbolos) y `comillas`.\n\n# no es un comentario",
    );
    let oid = create_commit(&git(), repo.path(), &message, false, &c())
        .await
        .expect("commit");

    let d = commit_details(&git(), repo.path(), &oid, &c())
        .await
        .expect("details");
    assert_eq!(d.summary, "Añade el archivo");
    assert_eq!(
        d.body,
        "Explica \"por qué\" | con $(símbolos) y `comillas`.\n\n# no es un comentario"
    );
    assert_eq!(d.parents, [ids["E"].clone()]);
    let s = working_status(&git(), repo.path(), &c())
        .await
        .expect("status");
    assert!(s.staged.is_empty());
    assert_eq!(s.untracked.len(), 1, "unstaged work is left alone");
}

#[tokio::test]
async fn amends_the_last_commit() {
    let (repo, ids) = TestRepo::sample_history().await;
    repo.write_file("late.txt", "x\n");
    repo.run(&["add", "late.txt"]).await;
    let info = amend_info(&git(), repo.path(), &c())
        .await
        .expect("info")
        .expect("head");
    assert_eq!(info.oid, ids["E"]);
    assert_eq!(info.summary, "E");
    assert!(info.published_in.is_empty());

    let oid = create_commit(&git(), repo.path(), "E, improved\n", true, &c())
        .await
        .expect("amend");
    assert_ne!(oid, ids["E"]);
    let d = commit_details(&git(), repo.path(), &oid, &c())
        .await
        .expect("details");
    assert_eq!(d.summary, "E, improved");
    assert_eq!(d.parents, [ids["M"].clone()], "replaces E, same parent");
}

#[tokio::test]
async fn reports_where_the_last_commit_was_published() {
    let (repo, _) = TestRepo::sample_history().await;
    let _origin = repo.add_bare_remote("origin").await;
    let info = amend_info(&git(), repo.path(), &c())
        .await
        .expect("info")
        .expect("head");
    assert_eq!(info.published_in, ["origin/main"]);

    repo.commit_file("local.txt", "x", "local only").await;
    let info = amend_info(&git(), repo.path(), &c())
        .await
        .expect("info")
        .expect("head");
    assert!(info.published_in.is_empty());
}

#[tokio::test]
async fn explains_common_failures() {
    let repo = TestRepo::new().await;
    assert!(
        amend_info(&git(), repo.path(), &c())
            .await
            .expect("info")
            .is_none()
    );

    let err = create_commit(&git(), repo.path(), "Nothing\n", false, &c())
        .await
        .expect_err("nothing staged");
    assert_eq!(err.kind(), ErrorKind::NothingToCommit, "{err}");

    repo.write_file("a.txt", "a");
    repo.run(&["add", "a.txt"]).await;
    let err = create_commit(&git(), repo.path(), "   \n", false, &c())
        .await
        .expect_err("empty message");
    assert_eq!(err.kind(), ErrorKind::EmptyMessage);
}

#[tokio::test]
async fn hooks_run_and_their_output_is_reported() {
    let repo = TestRepo::new().await;
    let hook = repo.write_file(
        ".git/hooks/pre-commit",
        "#!/bin/sh\necho 'lint failed: trailing spaces in a.txt' >&2\nexit 1\n",
    );
    // Git for Windows runs hooks through its sh without the executable bit.
    gitcraft_core::platform::set_file_mode(&hook, 0o755).expect("chmod");
    repo.write_file("a.txt", "a ");
    repo.run(&["add", "a.txt"]).await;
    let err = create_commit(&git(), repo.path(), "Try\n", false, &c())
        .await
        .expect_err("hook rejects");
    assert!(
        err.details().is_some_and(|d| d.contains("lint failed")),
        "{err}"
    );
}

#[tokio::test]
async fn identity_problems_are_recognised() {
    let repo = TestRepo::new().await;
    repo.write_file("a.txt", "a");
    repo.run(&["add", "a.txt"]).await;
    // No identity at all: isolated config and no GIT_AUTHOR_* variables.
    let out = Git::at(git().exe())
        .write(repo.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("EMAIL", "")
        .env("GIT_AUTHOR_NAME", "")
        .env("GIT_COMMITTER_NAME", "")
        .args(["-c", "user.useConfigOnly=true", "commit", "-q", "-m", "x"])
        .output(&c())
        .await
        .expect_err("no identity");
    assert_eq!(out.kind(), ErrorKind::IdentityUnknown, "{out}");
}
