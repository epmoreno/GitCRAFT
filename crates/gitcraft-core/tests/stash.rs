//! Stash entries against real repositories: nothing is forced, nothing is
//! dropped unless asked.

use std::path::PathBuf;

use gitcraft_core::ops::branch::BlockingFile;
use gitcraft_core::ops::commit::{commit_changes, commit_file_diff};
use gitcraft_core::ops::stash::{
    ApplyOutcome, apply_stash, create_stash, drop_stash, list_stashes, stash_and_apply,
};
use gitcraft_core::parse::diff::LineKind;
use gitcraft_core::parse::diff_tree::FileStatus;
use gitcraft_core::parse::stash::{STASH_FORMAT, StashOrigin, parse_stash_list};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind, Git};

fn git() -> Git {
    Git::discover().expect("git must be installed to run the tests")
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stash_list.z")
}

fn read(repo: &TestRepo, path: &str) -> Vec<u8> {
    std::fs::read(repo.path().join(path)).expect("read")
}

async fn messages(repo: &TestRepo) -> Vec<String> {
    repo.run(&["stash", "list", "--format=%gs"])
        .await
        .lines()
        .map(str::to_owned)
        .collect()
}

/// One entry of each origin: a user stash with untracked files, a discard
/// backup (stash create + store, as GitCRAFT makes it) and a "before
/// switching" stash made on another branch.
async fn repo_with_stashes() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.write_file("f.txt", "user change\n");
    repo.write_file("loose.txt", "untracked\n");
    repo.run(&["stash", "push", "--quiet", "-u", "-m", "mi trabajo"])
        .await;

    repo.write_file("f.txt", "discarded\n");
    let backup = repo
        .run(&["stash", "create", "GitCRAFT: descarte de f.txt"])
        .await;
    repo.run(&[
        "stash",
        "store",
        "-m",
        "GitCRAFT: descarte de f.txt",
        &backup,
    ])
    .await;
    repo.run(&["checkout", "--quiet", "--", "f.txt"]).await;

    repo.create_branch("feature/x").await;
    repo.checkout("feature/x").await;
    repo.write_file("f.txt", "before switching\n");
    repo.run(&[
        "stash",
        "push",
        "--quiet",
        "-m",
        "GitCRAFT: antes de cambiar a main",
    ])
    .await;
    repo
}

/// Regenerate with `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test stash`.
#[tokio::test]
async fn fixture_matches_current_git_output() {
    let repo = repo_with_stashes().await;
    let fresh = repo
        .read()
        .args(["stash", "list", "-z", STASH_FORMAT])
        .output(&c())
        .await
        .expect("stash list")
        .stdout_lossy();
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(fixture_path(), &fresh).expect("write fixture");
    }
    let fixture = std::fs::read_to_string(fixture_path()).expect("read fixture");
    assert!(
        fixture == fresh,
        "stash list output changed; see UPDATE_FIXTURES"
    );
}

#[test]
fn parses_the_stash_fixture() {
    let raw = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let list = parse_stash_list(&raw).expect("parse");
    let summary: Vec<_> = list
        .iter()
        .map(|e| {
            (
                e.index,
                e.message.as_str(),
                e.branch.as_deref(),
                e.origin,
                e.untracked.is_some(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                0,
                "GitCRAFT: antes de cambiar a main",
                Some("feature/x"),
                StashOrigin::BeforeSwitch,
                false
            ),
            (
                1,
                "GitCRAFT: descarte de f.txt",
                Some("main"),
                StashOrigin::Discard,
                false
            ),
            (2, "mi trabajo", Some("main"), StashOrigin::User, true),
        ]
    );
    assert!(list.iter().all(|e| e.oid.len() == 40 && e.time > 0));
}

#[tokio::test]
async fn creates_entries_with_a_message_and_untracked_files() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    let err = create_stash(&git(), repo.path(), "", false, &c())
        .await
        .expect_err("nothing to save");
    assert_eq!(err.kind(), ErrorKind::NothingToStash);

    repo.write_file("f.txt", "changed\n");
    repo.write_file("new.txt", "untracked\n");
    let oid = create_stash(&git(), repo.path(), "  probando  ", true, &c())
        .await
        .expect("stash");
    let list = list_stashes(&git(), repo.path(), &c()).await.expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].oid, oid);
    assert_eq!(list[0].message, "probando");
    assert!(list[0].untracked.is_some());
    assert_eq!(read(&repo, "f.txt"), b"base\n");
    assert!(!repo.path().join("new.txt").exists());

    // Without untracked files they stay put.
    repo.write_file("f.txt", "again\n");
    repo.write_file("other.txt", "stays\n");
    create_stash(&git(), repo.path(), "", false, &c())
        .await
        .expect("stash");
    assert_eq!(read(&repo, "other.txt"), b"stays\n");
    let list = list_stashes(&git(), repo.path(), &c()).await.expect("list");
    assert!(
        list[0].message.ends_with(" base"),
        "git names it after HEAD"
    );
}

#[tokio::test]
async fn applies_keeps_and_pops_by_commit_id() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.write_file("f.txt", "first\r\nno newline");
    let first = create_stash(&git(), repo.path(), "first", false, &c())
        .await
        .expect("stash");
    repo.write_file("g.txt", "second\n");
    create_stash(&git(), repo.path(), "second", true, &c())
        .await
        .expect("stash");
    // `first` is now stash@{1}: it is still found by its id.

    let outcome = apply_stash(&git(), repo.path(), &first, false, false, &c())
        .await
        .expect("apply");
    assert_eq!(outcome, ApplyOutcome::Applied { dropped: false });
    assert_eq!(read(&repo, "f.txt"), b"first\r\nno newline");
    assert_eq!(messages(&repo).await, ["On main: second", "On main: first"]);

    repo.run(&["checkout", "--quiet", "--", "f.txt"]).await;
    let outcome = apply_stash(&git(), repo.path(), &first, true, false, &c())
        .await
        .expect("pop");
    assert_eq!(outcome, ApplyOutcome::Applied { dropped: true });
    assert_eq!(read(&repo, "f.txt"), b"first\r\nno newline");
    assert_eq!(messages(&repo).await, ["On main: second"]);

    let err = apply_stash(&git(), repo.path(), &first, false, false, &c())
        .await
        .expect_err("gone");
    assert_eq!(err.kind(), ErrorKind::StashChanged);
}

#[tokio::test]
async fn never_overwrites_uncommitted_changes() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.write_file("f.txt", "stashed\n");
    repo.write_file("dir/u.txt", "stashed untracked\n");
    let oid = create_stash(&git(), repo.path(), "s", true, &c())
        .await
        .expect("stash");

    repo.write_file("f.txt", "local edit\n");
    repo.write_file("dir/u.txt", "local untracked\n");
    let index = repo.run(&["write-tree"]).await;
    let outcome = apply_stash(&git(), repo.path(), &oid, true, true, &c())
        .await
        .expect("apply");
    assert_eq!(
        outcome,
        ApplyOutcome::Blocked {
            files: vec![
                BlockingFile {
                    path: "f.txt".into(),
                    untracked: false
                },
                BlockingFile {
                    path: "dir/u.txt".into(),
                    untracked: true
                },
            ]
        }
    );
    assert_eq!(read(&repo, "f.txt"), b"local edit\n");
    assert_eq!(read(&repo, "dir/u.txt"), b"local untracked\n");
    assert_eq!(repo.run(&["write-tree"]).await, index);
    assert_eq!(messages(&repo).await.len(), 1, "entry kept");

    // "Save my changes, then apply": both versions are kept.
    let outcome = stash_and_apply(&git(), repo.path(), &oid, true, false, &c())
        .await
        .expect("stash and apply");
    assert_eq!(outcome, ApplyOutcome::Applied { dropped: true });
    assert_eq!(read(&repo, "f.txt"), b"stashed\n");
    assert_eq!(read(&repo, "dir/u.txt"), b"stashed untracked\n");
    let list = list_stashes(&git(), repo.path(), &c()).await.expect("list");
    assert_eq!(list.len(), 1);
    assert!(list[0].message.starts_with("GitCRAFT: antes de aplicar "));
    assert!(list[0].untracked.is_some());
    assert_eq!(
        repo.run(&["show", &format!("{}:f.txt", list[0].oid)]).await,
        "local edit"
    );
    assert_eq!(
        repo.run(&["show", &format!("{}^3:dir/u.txt", list[0].oid)])
            .await,
        "local untracked"
    );
}

#[tokio::test]
async fn reports_conflicts_before_touching_anything() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "1\n2\n3\n", "base").await;
    repo.write_file("f.txt", "1\nstash\n3\n");
    let oid = create_stash(&git(), repo.path(), "s", false, &c())
        .await
        .expect("stash");
    repo.commit_file("f.txt", "1\nbranch\n3\n", "branch").await;
    let head = repo.head().await;

    let outcome = apply_stash(&git(), repo.path(), &oid, true, false, &c())
        .await
        .expect("apply");
    assert_eq!(
        outcome,
        ApplyOutcome::WouldConflict {
            files: vec!["f.txt".into()],
            exact: true
        }
    );
    assert_eq!(read(&repo, "f.txt"), b"1\nbranch\n3\n");
    assert_eq!(repo.run(&["status", "--porcelain"]).await, "");

    // Accepted: conflict markers, and the entry is kept even for a pop.
    let outcome = apply_stash(&git(), repo.path(), &oid, true, true, &c())
        .await
        .expect("apply anyway");
    assert_eq!(
        outcome,
        ApplyOutcome::Conflicted {
            files: vec!["f.txt".into()]
        }
    );
    assert!(String::from_utf8_lossy(&read(&repo, "f.txt")).contains("<<<<<<<"));
    assert_eq!(messages(&repo).await, ["On main: s"]);
    assert_eq!(repo.head().await, head);
}

#[tokio::test]
async fn drops_only_the_named_entry_and_it_can_be_stored_back() {
    let repo = repo_with_stashes().await;
    let list = list_stashes(&git(), repo.path(), &c()).await.expect("list");
    let discard = list
        .iter()
        .find(|e| e.origin == StashOrigin::Discard)
        .expect("discard entry")
        .clone();
    drop_stash(&git(), repo.path(), &discard.oid, &c())
        .await
        .expect("drop");
    assert_eq!(
        messages(&repo).await,
        [
            "On feature/x: GitCRAFT: antes de cambiar a main",
            "On main: mi trabajo"
        ]
    );
    let err = drop_stash(&git(), repo.path(), &discard.oid, &c())
        .await
        .expect_err("already gone");
    assert_eq!(err.kind(), ErrorKind::StashChanged);

    // The recovery shown in the confirmation.
    repo.run(&["stash", "store", "-m", &discard.message, &discard.oid])
        .await;
    let list = list_stashes(&git(), repo.path(), &c()).await.expect("list");
    assert_eq!(list[0].oid, discard.oid);
    assert_eq!(list[0].origin, StashOrigin::Discard);
}

/// What the detail panel shows: the entry against its base, and its
/// untracked files as additions.
#[tokio::test]
async fn shows_an_entry_with_its_untracked_files() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "one\n", "base").await;
    repo.write_file("f.txt", "one\ntwo\n");
    repo.write_file("notes/new.md", "nuevo\n");
    create_stash(&git(), repo.path(), "con nuevos", true, &c())
        .await
        .expect("stash");
    let entry = list_stashes(&git(), repo.path(), &c())
        .await
        .expect("list")
        .remove(0);

    let tracked = commit_changes(&git(), repo.path(), &entry.oid, &c())
        .await
        .expect("changes");
    let paths: Vec<_> = tracked
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.status))
        .collect();
    assert_eq!(paths, [("f.txt", FileStatus::Modified)]);

    let untracked = entry.untracked.expect("untracked part");
    let added = commit_changes(&git(), repo.path(), &untracked, &c())
        .await
        .expect("untracked changes");
    let paths: Vec<_> = added
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.status))
        .collect();
    assert_eq!(paths, [("notes/new.md", FileStatus::Added)]);

    let diff = commit_file_diff(
        &git(),
        repo.path(),
        &untracked,
        "notes/new.md",
        None,
        false,
        &c(),
    )
    .await
    .expect("diff");
    let lines: Vec<_> = diff.hunks[0]
        .lines
        .iter()
        .map(|l| (l.kind, l.content.as_str()))
        .collect();
    assert_eq!(lines, [(LineKind::Added, "nuevo")]);
}
