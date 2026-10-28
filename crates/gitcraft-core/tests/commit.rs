//! Reading single commits from real repositories.

use gitcraft_core::ops::commit::commit_details;
use gitcraft_core::testing::{AUTHOR_EMAIL, AUTHOR_NAME, EPOCH, TestRepo};
use gitcraft_core::{CancellationToken, ErrorKind, Git, GitError};

fn git() -> Git {
    Git::discover().expect("git must be installed to run the tests")
}

#[tokio::test]
async fn reads_a_merge_commit() {
    let (repo, ids) = TestRepo::sample_history().await;
    let d = commit_details(&git(), repo.path(), &ids["M"], &CancellationToken::new())
        .await
        .expect("details");
    assert_eq!(d.oid, ids["M"]);
    assert_eq!(d.parents, [ids["B"].clone(), ids["D"].clone()]);
    assert_eq!(d.summary, "Merge branch 'feature'");
    assert_eq!(d.body, "");
    assert_eq!(d.author.name, AUTHOR_NAME);
    assert_eq!(d.author.email, AUTHOR_EMAIL);
    assert!(d.author.time > i64::try_from(EPOCH).expect("epoch"));
}

#[tokio::test]
async fn reads_a_body_and_a_different_committer() {
    let repo = TestRepo::new().await;
    let oid = repo
        .commit_with(&[
            "--author=José Ñúñez <jose@example.com>",
            "-m",
            "Subject",
            "-m",
            "First paragraph.\n\nSecond paragraph.",
        ])
        .await;
    let d = commit_details(&git(), repo.path(), &oid, &CancellationToken::new())
        .await
        .expect("details");
    assert_eq!(d.summary, "Subject");
    assert_eq!(d.body, "First paragraph.\n\nSecond paragraph.");
    assert_eq!(d.author.name, "José Ñúñez");
    assert_eq!(d.committer.name, AUTHOR_NAME);
    assert!(d.parents.is_empty(), "root commit");
}

#[tokio::test]
async fn ignores_show_signature_config() {
    let (repo, ids) = TestRepo::sample_history().await;
    repo.run(&["config", "log.showSignature", "true"]).await;
    let d = commit_details(&git(), repo.path(), &ids["E"], &CancellationToken::new())
        .await
        .expect("details");
    assert_eq!(d.summary, "E");
}

#[tokio::test]
async fn rejects_anything_but_an_object_id() {
    let (repo, _) = TestRepo::sample_history().await;
    for bad in ["HEAD", "--output=/tmp/x", "main", ""] {
        let err = commit_details(&git(), repo.path(), bad, &CancellationToken::new())
            .await
            .expect_err(bad);
        assert!(matches!(err, GitError::Parse(_)), "{bad}: {err}");
    }
    let missing = "0123456789012345678901234567890123456789";
    let err = commit_details(&git(), repo.path(), missing, &CancellationToken::new())
        .await
        .expect_err("unknown commit");
    assert_eq!(err.kind(), ErrorKind::Unknown);
    assert!(
        err.details()
            .is_some_and(|d| d.contains("bad object") || d.contains("unknown revision"))
    );
}

fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/show_details.z")
}

/// Real `git show -s` output for a commit with a body and a non-ASCII
/// author. Regenerate with `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test commit`.
#[tokio::test]
async fn details_fixture_matches_current_git_output() {
    use gitcraft_core::parse::commit::DETAILS_FORMAT;
    let repo = TestRepo::new().await;
    let oid = repo
        .commit_with(&[
            "--author=José Ñúñez <jose@example.com>",
            "-m",
            "Añade la vista\n\ncon cuerpo",
        ])
        .await;
    let fresh = repo
        .read()
        .args([
            "show",
            "-s",
            "--no-show-signature",
            "--no-notes",
            DETAILS_FORMAT,
            &oid,
            "--",
        ])
        .output(&CancellationToken::new())
        .await
        .expect("show")
        .stdout;
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(fixture_path(), &fresh).expect("write fixture");
    }
    assert!(std::fs::read(fixture_path()).expect("fixture") == fresh);
}

#[test]
fn parses_the_details_fixture() {
    let raw = std::fs::read(fixture_path()).expect("fixture");
    let d = gitcraft_core::parse::commit::parse_commit_details(&raw).expect("parse");
    assert_eq!(d.author.name, "José Ñúñez");
    assert_eq!(d.summary, "Añade la vista");
    assert_eq!(d.body, "con cuerpo");
    assert!(d.parents.is_empty());
}
