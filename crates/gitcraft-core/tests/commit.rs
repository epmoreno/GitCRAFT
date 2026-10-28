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

mod changes {
    use super::*;
    use gitcraft_core::ops::commit::commit_changes;
    use gitcraft_core::parse::diff_tree::{FileChange, FileStatus};

    fn find<'a>(files: &'a [FileChange], path: &str) -> &'a FileChange {
        files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("{path} not in {files:?}"))
    }

    /// A commit touching every kind of change.
    async fn mixed_commit() -> (TestRepo, String) {
        let repo = TestRepo::new().await;
        repo.write_file("keep.txt", "1\n2\n3\n");
        repo.write_file("remove.txt", "bye\n");
        repo.write_file(
            "old name.txt",
            "line one\nline two\nline three\nline four\nline five\n",
        );
        repo.commit_all("base").await;

        repo.write_file("keep.txt", "1\nTWO\n3\n4\n");
        std::fs::remove_file(repo.path().join("remove.txt")).expect("rm");
        std::fs::remove_file(repo.path().join("old name.txt")).expect("rm");
        repo.write_file(
            "dir/new name.txt",
            "line one\nline two\nline three\nline four\nline 5\n",
        );
        repo.write_file("ñandú.md", "hola\n");
        std::fs::write(repo.path().join("image.bin"), [0u8, 159, 146, 150, 0, 1]).expect("binary");
        let oid = repo.commit_all("mixed").await;
        (repo, oid)
    }

    #[tokio::test]
    async fn lists_every_kind_of_change_with_counts() {
        let (repo, oid) = mixed_commit().await;
        let changes = commit_changes(&git(), repo.path(), &oid, &CancellationToken::new())
            .await
            .expect("changes");
        assert!(changes.base.is_some());
        assert_eq!(changes.files.len(), 5, "{:?}", changes.files);

        let keep = find(&changes.files, "keep.txt");
        assert_eq!(keep.status, FileStatus::Modified);
        assert_eq!((keep.additions, keep.deletions), (Some(2), Some(1)));

        assert_eq!(
            find(&changes.files, "remove.txt").status,
            FileStatus::Deleted
        );

        let renamed = find(&changes.files, "dir/new name.txt");
        assert_eq!(renamed.status, FileStatus::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("old name.txt"));
        assert!(renamed.similarity.is_some_and(|s| s >= 50));
        assert_eq!((renamed.additions, renamed.deletions), (Some(1), Some(1)));

        assert_eq!(find(&changes.files, "ñandú.md").status, FileStatus::Added);
        let binary = find(&changes.files, "image.bin");
        assert_eq!((binary.additions, binary.deletions), (None, None));
    }

    #[tokio::test]
    async fn root_commits_compare_with_the_empty_tree() {
        let (repo, ids) = TestRepo::sample_history().await;
        let changes = commit_changes(&git(), repo.path(), &ids["A"], &CancellationToken::new())
            .await
            .expect("changes");
        assert_eq!(changes.base, None);
        assert_eq!(changes.files.len(), 1);
        assert_eq!(changes.files[0].path, "README.md");
        assert_eq!(changes.files[0].status, FileStatus::Added);
    }

    #[tokio::test]
    async fn merges_compare_with_their_first_parent() {
        let (repo, ids) = TestRepo::sample_history().await;
        let changes = commit_changes(&git(), repo.path(), &ids["M"], &CancellationToken::new())
            .await
            .expect("changes");
        assert_eq!(changes.base.as_deref(), Some(ids["B"].as_str()));
        // The merge brings feature.txt (from C and D) into main.
        let paths: Vec<_> = changes.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["feature.txt"]);
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    /// Real diff-tree output; regenerate with
    /// `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test commit`.
    #[tokio::test]
    async fn diff_tree_fixtures_match_current_git_output() {
        let (repo, oid) = mixed_commit().await;
        for (format, name) in [
            ("--name-status", "diff_tree_name_status.z"),
            ("--numstat", "diff_tree_numstat.z"),
        ] {
            let fresh = repo
                .read()
                .args(["diff-tree", "-r", "-z", "-M", "--no-commit-id", format])
                .arg(format!("{oid}^1"))
                .arg(&oid)
                .output(&CancellationToken::new())
                .await
                .expect("diff-tree")
                .stdout;
            if std::env::var_os("UPDATE_FIXTURES").is_some() {
                std::fs::write(fixture(name), &fresh).expect("write fixture");
            }
            assert!(
                std::fs::read(fixture(name)).expect("fixture") == fresh,
                "{name}"
            );
        }
    }

    #[test]
    fn parses_the_diff_tree_fixtures() {
        use gitcraft_core::parse::diff_tree::{merge_counts, parse_name_status, parse_numstat};
        let names = std::fs::read(fixture("diff_tree_name_status.z")).expect("fixture");
        let counts = std::fs::read(fixture("diff_tree_numstat.z")).expect("fixture");
        let files = merge_counts(
            parse_name_status(&names).expect("names"),
            &parse_numstat(&counts).expect("counts"),
        );
        assert_eq!(files.len(), 5);
        assert_eq!(
            find(&files, "dir/new name.txt").old_path.as_deref(),
            Some("old name.txt")
        );
        assert_eq!(find(&files, "image.bin").additions, None);
    }
}
