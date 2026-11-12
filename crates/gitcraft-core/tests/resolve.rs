//! Resolving files in conflict against real merges, rebases and
//! cherry-picks: exact bytes, nothing written over a file that changed,
//! and files marked resolved put back in conflict only when git can do it
//! faithfully.

use gitcraft_core::ops::conflict::conflict_file;
use gitcraft_core::ops::resolve::{
    MarkOutcome, ResolvedFile, Side, choose_version, mark_resolved, resolve_by_deleting,
    resolved_files, restart_file, unmark_resolved, write_resolution,
};
use gitcraft_core::parse::conflict::MarkerLine;
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind};

fn c() -> CancellationToken {
    CancellationToken::new()
}

fn write(repo: &TestRepo, path: &str, bytes: &[u8]) {
    let full = repo.path().join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("dirs");
    }
    std::fs::write(full, bytes).expect("write");
}

fn read(repo: &TestRepo, path: &str) -> Vec<u8> {
    std::fs::read(repo.path().join(path)).expect("read")
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

/// `path` goes `base` → `main` on main and `base` → `feature` on feature;
/// merging feature into main stops. `config` is set on the new repo first.
async fn merge_conflict_with(
    path: &str,
    base: &[u8],
    main: &[u8],
    feature: &[u8],
    config: &[(&str, &str)],
) -> TestRepo {
    let repo = TestRepo::new().await;
    for (key, value) in config {
        repo.run(&["config", key, value]).await;
    }
    write(&repo, path, base);
    repo.commit_all("base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    write(&repo, path, feature);
    repo.commit_all("feature").await;
    repo.checkout("main").await;
    write(&repo, path, main);
    repo.commit_all("main").await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;
    repo
}

async fn merge_conflict(path: &str) -> TestRepo {
    merge_conflict_with(path, b"base\n", b"main\n", b"feature\n", &[]).await
}

async fn fingerprint(repo: &TestRepo, path: &str) -> Option<String> {
    conflict_file(repo.git(), repo.path(), path, &c())
        .await
        .expect("conflict_file")
        .fingerprint
}

async fn unmerged(repo: &TestRepo) -> String {
    repo.run(&["ls-files", "-u"]).await
}

async fn resolve(repo: &TestRepo, path: &str, contents: &str) {
    let print = fingerprint(repo, path).await.expect("a file");
    write_resolution(repo.git(), repo.path(), path, contents, &print, &c())
        .await
        .expect("write_resolution");
    let outcome = mark_resolved(repo.git(), repo.path(), path, false, &c())
        .await
        .expect("mark_resolved");
    assert_eq!(outcome, MarkOutcome::Resolved);
}

#[tokio::test]
async fn writes_the_result_byte_for_byte_and_marks_it() {
    let repo = merge_conflict("f.txt").await;
    let print = fingerprint(&repo, "f.txt").await.expect("a file");
    let new_print = write_resolution(
        repo.git(),
        repo.path(),
        "f.txt",
        "main\nfeature\n",
        &print,
        &c(),
    )
    .await
    .expect("write");
    assert_eq!(read(&repo, "f.txt"), b"main\nfeature\n");
    assert_eq!(Some(new_print), fingerprint(&repo, "f.txt").await);
    // Writing does not mark it resolved.
    assert!(!unmerged(&repo).await.is_empty());

    let outcome = mark_resolved(repo.git(), repo.path(), "f.txt", false, &c())
        .await
        .expect("mark");
    assert_eq!(outcome, MarkOutcome::Resolved);
    assert!(unmerged(&repo).await.is_empty());
    assert_eq!(repo.run(&["show", ":f.txt"]).await, "main\nfeature");
    // No temporary file is left next to it.
    assert_eq!(
        repo.run(&["status", "--porcelain", "-uall"]).await,
        "M  f.txt"
    );
}

#[tokio::test]
async fn crlf_results_keep_their_bytes() {
    let crlf = |s: &str| s.replace('\n', "\r\n").into_bytes();
    let repo = merge_conflict_with(
        "f.txt",
        &crlf("base\n"),
        &crlf("main\n"),
        &crlf("feature\n"),
        &[],
    )
    .await;
    resolve(&repo, "f.txt", "main\r\nfeature\r\n").await;
    assert_eq!(read(&repo, "f.txt"), b"main\r\nfeature\r\n");
}

/// With `core.autocrlf=true` the working tree has CRLF and git stores LF.
#[tokio::test]
async fn autocrlf_results_are_normalised_by_git() {
    let repo = merge_conflict_with(
        "f.txt",
        b"base\n",
        b"main\n",
        b"feature\n",
        &[("core.autocrlf", "true")],
    )
    .await;
    assert!(read(&repo, "f.txt").windows(2).any(|w| w == b"\r\n"));
    resolve(&repo, "f.txt", "main\r\nfeature\r\n").await;
    assert_eq!(read(&repo, "f.txt"), b"main\r\nfeature\r\n");
    let stored = repo
        .read()
        .args(["cat-file", "blob", ":f.txt"])
        .output(&c())
        .await
        .expect("cat-file");
    assert_eq!(stored.stdout, b"main\nfeature\n");
}

#[cfg(unix)]
#[tokio::test]
async fn keeps_the_executable_bit() {
    use std::os::unix::fs::PermissionsExt;
    let repo = TestRepo::new().await;
    write(&repo, "run.sh", b"base\n");
    std::fs::set_permissions(
        repo.path().join("run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("chmod");
    repo.commit_all("base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    write(&repo, "run.sh", b"feature\n");
    repo.commit_all("feature").await;
    repo.checkout("main").await;
    write(&repo, "run.sh", b"main\n");
    repo.commit_all("main").await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;

    resolve(&repo, "run.sh", "both\n").await;
    let mode = std::fs::metadata(repo.path().join("run.sh"))
        .expect("meta")
        .permissions()
        .mode();
    assert_eq!(mode & 0o111, 0o111);
    assert!(
        repo.run(&["ls-files", "-s", "run.sh"])
            .await
            .starts_with("100755")
    );
}

#[tokio::test]
async fn nothing_is_written_over_a_file_that_changed() {
    let repo = merge_conflict("f.txt").await;
    let print = fingerprint(&repo, "f.txt").await;
    // Edited in another editor after the view read it.
    write(&repo, "f.txt", b"edited elsewhere\n");

    let err = write_resolution(
        repo.git(),
        repo.path(),
        "f.txt",
        "mine\n",
        print.as_deref().expect("a file"),
        &c(),
    )
    .await
    .expect_err("stale");
    assert_eq!(err.kind(), ErrorKind::StaleDiff);
    for result in [
        choose_version(
            repo.git(),
            repo.path(),
            "f.txt",
            Side::Ours,
            print.as_deref(),
            &c(),
        )
        .await
        .map(drop),
        restart_file(repo.git(), repo.path(), "f.txt", print.as_deref(), &c())
            .await
            .map(drop),
        resolve_by_deleting(repo.git(), repo.path(), "f.txt", print.as_deref(), &c()).await,
    ] {
        assert_eq!(result.expect_err("stale").kind(), ErrorKind::StaleDiff);
    }
    assert_eq!(read(&repo, "f.txt"), b"edited elsewhere\n");
    assert!(!unmerged(&repo).await.is_empty());
}

#[tokio::test]
async fn binary_files_keep_one_version() {
    for (side, expected) in [(Side::Ours, b"\0main"), (Side::Theirs, b"\0feat")] {
        let repo = merge_conflict_with("img.bin", b"\0base", b"\0main", b"\0feat", &[]).await;
        let print = fingerprint(&repo, "img.bin").await;
        choose_version(
            repo.git(),
            repo.path(),
            "img.bin",
            side,
            print.as_deref(),
            &c(),
        )
        .await
        .expect("choose");
        assert_eq!(read(&repo, "img.bin"), expected);
        let outcome = mark_resolved(repo.git(), repo.path(), "img.bin", false, &c())
            .await
            .expect("mark");
        assert_eq!(outcome, MarkOutcome::Resolved);
        assert!(unmerged(&repo).await.is_empty());
    }
}

/// `f.txt` deleted on main (ours) and modified on feature, or the other
/// way round.
async fn modify_delete(deleted_on_main: bool) -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    if deleted_on_main {
        repo.commit_file("f.txt", "feature\n", "feature").await;
    } else {
        repo.run(&["rm", "--quiet", "--", "f.txt"]).await;
        repo.commit_all("delete").await;
    }
    repo.checkout("main").await;
    if deleted_on_main {
        repo.run(&["rm", "--quiet", "--", "f.txt"]).await;
        repo.commit_all("delete").await;
    } else {
        repo.commit_file("f.txt", "main\n", "main").await;
    }
    stop_on_conflict(&repo, &["merge", "feature"]).await;
    repo
}

#[tokio::test]
async fn modified_and_deleted_both_ways() {
    // Deleted on our side: keep their version.
    let repo = modify_delete(true).await;
    let print = fingerprint(&repo, "f.txt").await;
    choose_version(
        repo.git(),
        repo.path(),
        "f.txt",
        Side::Theirs,
        print.as_deref(),
        &c(),
    )
    .await
    .expect("keep theirs");
    let err = choose_version(repo.git(), repo.path(), "f.txt", Side::Ours, None, &c())
        .await
        .expect_err("there is no version of ours");
    assert_eq!(err.kind(), ErrorKind::Unknown);
    mark_resolved(repo.git(), repo.path(), "f.txt", false, &c())
        .await
        .expect("mark");
    assert_eq!(read(&repo, "f.txt"), b"feature\n");
    assert!(unmerged(&repo).await.is_empty());

    // Deleted on their side: delete it.
    let repo = modify_delete(false).await;
    let print = fingerprint(&repo, "f.txt").await;
    resolve_by_deleting(repo.git(), repo.path(), "f.txt", print.as_deref(), &c())
        .await
        .expect("delete");
    assert!(!repo.path().join("f.txt").exists());
    assert!(unmerged(&repo).await.is_empty());
    assert_eq!(repo.run(&["ls-files", "--", "f.txt"]).await, "");
}

#[tokio::test]
async fn leftover_markers_need_force() {
    let repo = merge_conflict("f.txt").await;
    let outcome = mark_resolved(repo.git(), repo.path(), "f.txt", false, &c())
        .await
        .expect("mark");
    let line = |line, text: &str| MarkerLine {
        line,
        text: text.to_owned(),
    };
    assert_eq!(
        outcome,
        MarkOutcome::MarkersLeft {
            lines: vec![
                line(1, "<<<<<<< HEAD"),
                line(3, "======="),
                line(5, ">>>>>>> feature"),
            ]
        }
    );
    assert!(!unmerged(&repo).await.is_empty(), "nothing was done");

    let outcome = mark_resolved(repo.git(), repo.path(), "f.txt", true, &c())
        .await
        .expect("force");
    assert_eq!(outcome, MarkOutcome::Resolved);
    assert!(unmerged(&repo).await.is_empty());
}

#[tokio::test]
async fn restart_puts_the_markers_back() {
    let repo = merge_conflict("f.txt").await;
    let print = fingerprint(&repo, "f.txt").await.expect("a file");
    let print = write_resolution(repo.git(), repo.path(), "f.txt", "half\n", &print, &c())
        .await
        .expect("write");
    restart_file(repo.git(), repo.path(), "f.txt", Some(&print), &c())
        .await
        .expect("restart");
    let text = String::from_utf8(read(&repo, "f.txt")).expect("utf-8");
    assert!(text.contains("main\n=======\nfeature\n"), "{text}");
    assert!(!unmerged(&repo).await.is_empty());
}

#[tokio::test]
async fn unusual_paths() {
    for path in ["con espacio/ñandú.txt", "-dash.txt", "[glob].txt"] {
        let repo = merge_conflict(path).await;
        resolve(&repo, path, "both\n").await;
        assert_eq!(read(&repo, path), b"both\n", "{path}");
        assert!(unmerged(&repo).await.is_empty(), "{path}");
    }
}

// --- Putting a file marked resolved back in conflict -----------------------

async fn listed(repo: &TestRepo) -> Vec<ResolvedFile> {
    resolved_files(repo.git(), repo.path(), &c())
        .await
        .expect("resolved_files")
}

#[tokio::test]
async fn unmark_restores_the_conflict_and_leaves_the_file() {
    let repo = merge_conflict("f.txt").await;
    let before = unmerged(&repo).await;
    resolve(&repo, "f.txt", "resolved\n").await;
    assert_eq!(
        listed(&repo).await,
        vec![ResolvedFile {
            path: "f.txt".into(),
            changed_since: false
        }]
    );

    unmark_resolved(repo.git(), repo.path(), "f.txt", &c())
        .await
        .expect("unmark");
    assert_eq!(unmerged(&repo).await, before);
    assert_eq!(read(&repo, "f.txt"), b"resolved\n");
    assert!(listed(&repo).await.is_empty());
}

#[tokio::test]
async fn edits_after_marking_are_reported() {
    let repo = merge_conflict("f.txt").await;
    resolve(&repo, "f.txt", "resolved\n").await;
    write(&repo, "f.txt", b"edited after\n");
    assert_eq!(
        listed(&repo).await,
        vec![ResolvedFile {
            path: "f.txt".into(),
            changed_since: true
        }]
    );
}

#[tokio::test]
async fn unmark_a_deleted_file() {
    let repo = modify_delete(false).await;
    let before = unmerged(&repo).await;
    let print = fingerprint(&repo, "f.txt").await;
    resolve_by_deleting(repo.git(), repo.path(), "f.txt", print.as_deref(), &c())
        .await
        .expect("delete");
    unmark_resolved(repo.git(), repo.path(), "f.txt", &c())
        .await
        .expect("unmark");
    assert_eq!(unmerged(&repo).await, before);
}

#[tokio::test]
async fn nothing_to_unmark_once_the_operation_ends() {
    let repo = merge_conflict("f.txt").await;
    resolve(&repo, "f.txt", "resolved\n").await;
    repo.run(&["commit", "--quiet", "--no-edit"]).await;
    // git still keeps the old versions, but there is no conflict to go back
    // to.
    assert!(listed(&repo).await.is_empty());
    let err = unmark_resolved(repo.git(), repo.path(), "f.txt", &c())
        .await
        .expect_err("ended");
    assert_eq!(err.kind(), ErrorKind::CannotUnmark);
    assert!(unmerged(&repo).await.is_empty());

    // Nor during a later operation that has nothing to do with that file.
    repo.create_branch("other").await;
    repo.checkout("other").await;
    repo.commit_file("g.txt", "other\n", "other").await;
    repo.checkout("main").await;
    repo.commit_file("g.txt", "main\n", "main g").await;
    stop_on_conflict(&repo, &["merge", "other"]).await;
    assert!(listed(&repo).await.is_empty());
    let err = unmark_resolved(repo.git(), repo.path(), "f.txt", &c())
        .await
        .expect_err("not this operation");
    assert_eq!(err.kind(), ErrorKind::CannotUnmark);
}

#[tokio::test]
async fn unmark_during_a_rebase_only_for_the_current_commit() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "base\n", "base").await;
    repo.commit_file("c.txt", "base\n", "base c").await;
    repo.create_branch("topic").await;
    write(&repo, "a.txt", b"main\n");
    write(&repo, "c.txt", b"main\n");
    repo.commit_all("main").await;
    repo.checkout("topic").await;
    repo.commit_file("a.txt", "t1\n", "t1").await;
    repo.commit_file("c.txt", "t2\n", "t2").await;
    stop_on_conflict(&repo, &["rebase", "main"]).await;

    resolve(&repo, "a.txt", "resolved\n").await;
    assert_eq!(listed(&repo).await.len(), 1);
    let out = repo
        .write()
        .env("GIT_EDITOR", "true")
        .args(["rebase", "--continue"])
        .accept_exit_code(1)
        .output(&c())
        .await
        .expect("continue");
    assert_eq!(out.exit_code, Some(1), "stops at the second commit");
    // a.txt belongs to the commit already replayed.
    assert!(listed(&repo).await.is_empty());

    let before = unmerged(&repo).await;
    resolve(&repo, "c.txt", "resolved\n").await;
    unmark_resolved(repo.git(), repo.path(), "c.txt", &c())
        .await
        .expect("unmark");
    assert_eq!(unmerged(&repo).await, before);
}

#[tokio::test]
async fn unmark_during_a_cherry_pick() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    let pick = repo.commit_file("f.txt", "feature\n", "feature").await;
    repo.checkout("main").await;
    repo.commit_file("f.txt", "main\n", "main").await;
    stop_on_conflict(&repo, &["cherry-pick", &pick]).await;

    let before = unmerged(&repo).await;
    resolve(&repo, "f.txt", "resolved\n").await;
    unmark_resolved(repo.git(), repo.path(), "f.txt", &c())
        .await
        .expect("unmark");
    assert_eq!(unmerged(&repo).await, before);
}
