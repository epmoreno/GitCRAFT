//! Reading single commits from real repositories.

use gitcraft_core::ops::commit::commit_details;
use gitcraft_core::testing::{AUTHOR_EMAIL, AUTHOR_NAME, EPOCH, TestRepo};
use gitcraft_core::{CancellationToken, ErrorKind, Git, GitError};

fn git() -> Git {
    gitcraft_core::testing::isolated_git()
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

mod diffs {
    use super::*;
    use gitcraft_core::ops::commit::{DIFF_LINE_LIMIT, commit_file_diff};
    use gitcraft_core::parse::diff::{LineKind, parse_diff};

    async fn diff(
        repo: &TestRepo,
        oid: &str,
        path: &str,
        old: Option<&str>,
        full: bool,
    ) -> gitcraft_core::parse::diff::FileDiff {
        commit_file_diff(
            &git(),
            repo.path(),
            oid,
            path,
            old,
            full,
            &CancellationToken::new(),
        )
        .await
        .expect("diff")
    }

    #[tokio::test]
    async fn diffs_a_modified_file_with_line_numbers() {
        let repo = TestRepo::new().await;
        repo.commit_file("f.txt", "one\ntwo\nthree\n", "base").await;
        let oid = repo
            .commit_file("f.txt", "one\nTWO\nthree\nfour\n", "edit")
            .await;
        let d = diff(&repo, &oid, "f.txt", None, false).await;
        assert!(!d.binary && !d.too_large);
        let lines: Vec<_> = d.hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.content.as_str(), l.old_line, l.new_line))
            .collect();
        assert_eq!(
            lines,
            [
                (LineKind::Context, "one", Some(1), Some(1)),
                (LineKind::Removed, "two", Some(2), None),
                (LineKind::Added, "TWO", None, Some(2)),
                (LineKind::Context, "three", Some(3), Some(3)),
                (LineKind::Added, "four", None, Some(4)),
            ]
        );
    }

    #[tokio::test]
    async fn diffs_root_commits_renames_and_binaries() {
        let (repo, ids) = TestRepo::sample_history().await;
        let d = diff(&repo, &ids["A"], "README.md", None, false).await;
        assert_eq!(d.notes, ["new file mode 100644"]);
        assert_eq!(d.hunks[0].lines[0].content, "# Sample");

        repo.write_file("a/long.txt", "1\n2\n3\n4\n5\n6\n7\n8\n");
        repo.commit_all("add").await;
        std::fs::rename(repo.path().join("a/long.txt"), repo.path().join("b.txt")).expect("mv");
        repo.write_file("b.txt", "1\n2\n3\n4\n5\n6\n7\nEIGHT\n");
        let renamed = repo.commit_all("rename").await;
        let d = diff(&repo, &renamed, "b.txt", Some("a/long.txt"), false).await;
        assert!(
            d.notes.iter().any(|n| n == "rename from a/long.txt"),
            "{:?}",
            d.notes
        );
        assert_eq!(
            d.line_count, 5,
            "only the changed tail, not a full add+delete"
        );

        std::fs::write(repo.path().join("img.bin"), [0u8, 1, 2, 0, 255]).expect("bin");
        let bin = repo.commit_all("binary").await;
        assert!(diff(&repo, &bin, "img.bin", None, false).await.binary);
    }

    #[tokio::test]
    async fn large_diffs_are_only_sent_on_request() {
        let repo = TestRepo::new().await;
        let big: String = (0..DIFF_LINE_LIMIT + 10)
            .map(|i| format!("line {i}\n"))
            .collect();
        let oid = repo.commit_file("big.txt", &big, "big").await;
        let d = diff(&repo, &oid, "big.txt", None, false).await;
        assert!(d.too_large);
        assert!(d.hunks.is_empty());
        assert_eq!(d.line_count, DIFF_LINE_LIMIT + 10);
        let d = diff(&repo, &oid, "big.txt", None, true).await;
        assert!(!d.too_large);
        assert_eq!(d.hunks[0].lines.len() as u32, DIFF_LINE_LIMIT + 10);
    }

    /// A file name git would take as a glob if paths were not literal.
    /// Windows does not allow `*` in file names; `[ab]` is a glob too, and
    /// would match a.txt.
    const GLOB_NAME: &str = if cfg!(windows) { "[ab].txt" } else { "*.txt" };

    #[tokio::test]
    async fn paths_are_literal_never_globs_or_magic() {
        let repo = TestRepo::new().await;
        repo.write_file("a.txt", "a\n");
        repo.write_file(GLOB_NAME, "star\n");
        // Windows does not allow ':' in file names.
        #[cfg(not(windows))]
        repo.write_file(":(icase)X", "magic\n");
        repo.write_file("-dash", "dash\n");
        let oid = repo.commit_all("odd names").await;

        let star = diff(&repo, &oid, GLOB_NAME, None, false).await;
        assert_eq!(
            star.hunks.len(),
            1,
            "only the file literally named {GLOB_NAME}"
        );
        assert_eq!(star.hunks[0].lines[0].content, "star");
        #[cfg(not(windows))]
        assert_eq!(
            diff(&repo, &oid, ":(icase)X", None, false).await.hunks[0].lines[0].content,
            "magic"
        );
        // As magic, this would name a.txt; taken literally it names nothing.
        let magic = commit_file_diff(
            &git(),
            repo.path(),
            &oid,
            ":(icase)A.TXT",
            None,
            false,
            &CancellationToken::new(),
        )
        .await;
        assert!(!matches!(&magic, Ok(d) if !d.hunks.is_empty()), "{magic:?}");
        assert_eq!(
            diff(&repo, &oid, "-dash", None, false).await.hunks[0].lines[0].content,
            "dash"
        );
    }

    #[tokio::test]
    async fn rejects_bad_input() {
        let (repo, ids) = TestRepo::sample_history().await;
        let bad = |oid: &'static str, path: &'static str| {
            let repo = &repo;
            async move {
                commit_file_diff(
                    &git(),
                    repo.path(),
                    oid,
                    path,
                    None,
                    false,
                    &CancellationToken::new(),
                )
                .await
                .is_err()
            }
        };
        assert!(bad("HEAD", "README.md").await);
        assert!(
            commit_file_diff(
                &git(),
                repo.path(),
                &ids["A"],
                "",
                None,
                false,
                &CancellationToken::new()
            )
            .await
            .is_err()
        );
    }

    fn fixture() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/diff_modified.patch")
    }

    /// Real `diff-tree -p` output; regenerate with
    /// `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test commit`.
    #[tokio::test]
    async fn diff_fixture_matches_current_git_output() {
        let repo = TestRepo::new().await;
        repo.commit_file(
            "f.rs",
            "fn main() {\n    a();\n    b();\n}\n\nfn tail() {}",
            "base",
        )
        .await;
        let oid = repo
            .commit_file(
                "f.rs",
                "fn main() {\n    a();\n    c();\n}\n\nfn tail() { done() }\n",
                "edit",
            )
            .await;
        let fresh = repo
            .read()
            .args([
                "diff-tree",
                "-p",
                "-M",
                "-U3",
                "--no-ext-diff",
                "--no-commit-id",
            ])
            .arg(format!("{oid}^1"))
            .arg(&oid)
            .output(&CancellationToken::new())
            .await
            .expect("diff-tree")
            .stdout;
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(fixture(), &fresh).expect("write");
        }
        assert!(std::fs::read(fixture()).expect("fixture") == fresh);
    }

    #[test]
    fn parses_the_diff_fixture() {
        let d = parse_diff(&std::fs::read(fixture()).expect("fixture")).expect("parse");
        let all: Vec<_> = d.hunks.iter().flat_map(|h| &h.lines).collect();
        assert!(
            all.iter()
                .any(|l| l.kind == LineKind::Removed && l.content == "    b();")
        );
        assert!(
            all.iter()
                .any(|l| l.kind == LineKind::Added && l.content == "    c();")
        );
        let tail = all
            .iter()
            .find(|l| l.content == "fn tail() {}")
            .expect("old tail");
        assert!(tail.no_newline, "the old version had no final newline");
    }
}
