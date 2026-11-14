//! Resetting the current branch: what the preview lists, the copy made
//! before (stash, Trash), and an exact undo.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gitcraft_core::ops::discard::Trash;
use gitcraft_core::ops::reset::{
    ResetMode, ResetPreview, ResetRefusal, reset, reset_preview, reset_stash_message, undo_reset,
};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind};

fn c() -> CancellationToken {
    CancellationToken::new()
}

/// A "Trash" in a temporary directory, so tests never touch the real one.
struct FakeTrash {
    dir: tempfile::TempDir,
    count: Mutex<u32>,
}

impl FakeTrash {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("tempdir"),
            count: Mutex::new(0),
        }
    }
}

impl Trash for FakeTrash {
    fn trash(&self, path: &Path) -> io::Result<PathBuf> {
        let mut count = self.count.lock().expect("lock");
        *count += 1;
        let to = self.dir.path().join(format!("{count}"));
        std::fs::rename(path, &to)?;
        Ok(to)
    }
}

/// HEAD, the branch, the index tree and every file outside .git.
async fn snapshot(repo: &TestRepo) -> (String, String, String, BTreeMap<String, Vec<u8>>) {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("list") {
            let path = entry.expect("entry").path();
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("inside")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.insert(rel, std::fs::read(&path).expect("read"));
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(repo.path(), repo.path(), &mut files);
    (
        repo.head().await,
        repo.run(&["symbolic-ref", "--short", "HEAD"]).await,
        repo.run(&["write-tree"]).await,
        files,
    )
}

/// main: c1 (f = 1) → c2 (f = 2) → c3 (f = 3, g.txt added).
async fn history() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "1\n", "c1").await;
    repo.commit_file("f.txt", "2\n", "c2").await;
    repo.write_file("f.txt", "3\n");
    repo.write_file("g.txt", "g\n");
    repo.commit_all("c3").await;
    repo
}

async fn preview(repo: &TestRepo, rev: &str, mode: ResetMode) -> ResetPreview {
    let oid = repo.rev_parse(rev).await;
    reset_preview(repo.git(), repo.path(), &oid, mode, &c())
        .await
        .expect("preview")
}

#[tokio::test]
async fn the_preview_lists_the_commits_that_leave_the_branch() {
    let repo = history().await;
    let _remote = repo.add_bare_remote("origin").await;
    // The test remote's main goes back to c2 (c3 is only local); c2 is also
    // on a tag.
    repo.run(&[
        "push",
        "--quiet",
        "--force",
        "origin",
        "HEAD~1:refs/heads/main",
    ])
    .await;
    repo.run(&["fetch", "--quiet", "origin"]).await;
    repo.run(&["tag", "keep", "HEAD~1"]).await;

    let p = preview(&repo, "HEAD~2", ResetMode::Soft).await;
    assert_eq!(p.refused, None);
    assert_eq!(p.branch.as_deref(), Some("main"));
    assert_eq!(p.target.subject, "c1");
    assert_eq!(p.leaving_total, 2);
    let summary: Vec<(&str, bool, bool)> = p
        .leaving
        .iter()
        .map(|l| (l.commit.subject.as_str(), l.only_here, l.published))
        .collect();
    assert_eq!(summary, vec![("c3", true, false), ("c2", false, true)]);
    assert_eq!(p.published_in, vec!["origin/main".to_owned()]);
}

#[tokio::test]
async fn soft_keeps_the_changes_staged_and_undoes_exactly() {
    let repo = history().await;
    repo.write_file("f.txt", "work in progress\n");
    let before = snapshot(&repo).await;
    let p = preview(&repo, "HEAD~1", ResetMode::Soft).await;
    assert!(!p.saves_stash && p.lost_changes.is_empty());

    let trash = FakeTrash::new();
    let record = reset(
        repo.git(),
        repo.path(),
        &p.target.oid,
        ResetMode::Soft,
        &p.fingerprint,
        &trash,
        &c(),
    )
    .await
    .expect("reset");
    assert_eq!(repo.head().await, p.target.oid);
    assert_eq!(record.stash, None);
    // c3's changes are staged; the working tree did not move.
    assert!(
        repo.run(&["diff", "--cached", "--name-only"])
            .await
            .contains("g.txt")
    );
    assert_eq!(snapshot(&repo).await.3, before.3);

    undo_reset(repo.git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(snapshot(&repo).await, before);
}

#[tokio::test]
async fn mixed_saves_the_staged_work_and_undoes_the_index() {
    let repo = history().await;
    repo.write_file("f.txt", "staged\n");
    repo.run(&["add", "f.txt"]).await;
    repo.write_file("f.txt", "staged, then edited\n");
    let before = snapshot(&repo).await;
    let p = preview(&repo, "HEAD~1", ResetMode::Mixed).await;
    assert_eq!(p.unstaged_by_reset, vec!["f.txt".to_owned()]);
    assert!(p.saves_stash);

    let trash = FakeTrash::new();
    let record = reset(
        repo.git(),
        repo.path(),
        &p.target.oid,
        ResetMode::Mixed,
        &p.fingerprint,
        &trash,
        &c(),
    )
    .await
    .expect("reset");
    let stash = record.stash.clone().expect("a copy");
    // The staged version is in the copy's index.
    assert_eq!(
        repo.run(&["show", &format!("{stash}^2:f.txt")]).await,
        "staged"
    );
    assert_eq!(repo.run(&["diff", "--cached", "--name-only"]).await, "");

    undo_reset(repo.git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(snapshot(&repo).await, before);
}

#[tokio::test]
async fn hard_copies_everything_it_would_lose_and_undoes_exactly() {
    // c1 tracks h.txt and c2 deletes it: an untracked h.txt at c2 is
    // overwritten by reset --hard to c1.
    let repo = TestRepo::new().await;
    repo.write_file("f.txt", "1\n");
    repo.write_file("h.txt", "tracked in c1\n");
    repo.commit_all("c1").await;
    repo.run(&["rm", "--quiet", "h.txt"]).await;
    repo.commit_file("f.txt", "2\n", "c2").await;
    repo.write_file("f.txt", "uncommitted\n");
    repo.write_file("h.txt", "my untracked h\n");
    repo.write_file("notes.txt", "untracked, not in the way\n");
    let before = snapshot(&repo).await;

    let p = preview(&repo, "HEAD~1", ResetMode::Hard).await;
    assert_eq!(p.lost_changes, vec!["f.txt".to_owned()]);
    assert_eq!(p.overwritten_untracked, vec!["h.txt".to_owned()]);
    assert!(p.saves_stash);

    let trash = FakeTrash::new();
    let record = reset(
        repo.git(),
        repo.path(),
        &p.target.oid,
        ResetMode::Hard,
        &p.fingerprint,
        &trash,
        &c(),
    )
    .await
    .expect("reset");
    // The working tree is c1; the copy and the Trash have the rest.
    assert_eq!(
        std::fs::read_to_string(repo.path().join("f.txt")).expect("f"),
        "1\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("h.txt")).expect("h"),
        "tracked in c1\n"
    );
    let stash = record.stash.clone().expect("a copy");
    assert_eq!(
        repo.run(&["show", &format!("{stash}:f.txt")]).await,
        "uncommitted"
    );
    assert_eq!(
        repo.run(&["stash", "list", "--format=%gs"]).await,
        reset_stash_message(&p.target.short)
    );
    assert_eq!(record.trashed.len(), 1);
    assert_eq!(
        std::fs::read_to_string(&record.trashed[0].1).expect("in the trash"),
        "my untracked h\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("notes.txt")).expect("notes"),
        "untracked, not in the way\n"
    );

    undo_reset(repo.git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(snapshot(&repo).await, before);
}

#[tokio::test]
async fn nothing_happens_if_the_repository_changed_since_the_preview() {
    let repo = history().await;
    let before_head = repo.head().await;
    let p = preview(&repo, "HEAD~1", ResetMode::Hard).await;
    repo.write_file("f.txt", "edited after the preview\n");
    let trash = FakeTrash::new();
    let err = reset(
        repo.git(),
        repo.path(),
        &p.target.oid,
        ResetMode::Hard,
        &p.fingerprint,
        &trash,
        &c(),
    )
    .await
    .expect_err("stale");
    assert_eq!(err.kind(), ErrorKind::StaleDiff);
    assert_eq!(repo.head().await, before_head);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("f.txt")).expect("f"),
        "edited after the preview\n"
    );
}

#[tokio::test]
async fn undo_refuses_once_something_changed() {
    let repo = history().await;
    let p = preview(&repo, "HEAD~1", ResetMode::Hard).await;
    let trash = FakeTrash::new();
    let record = reset(
        repo.git(),
        repo.path(),
        &p.target.oid,
        ResetMode::Hard,
        &p.fingerprint,
        &trash,
        &c(),
    )
    .await
    .expect("reset");
    repo.write_file("f.txt", "new work after the reset\n");
    let err = undo_reset(repo.git(), repo.path(), &record, &c())
        .await
        .expect_err("changed");
    assert_eq!(err.kind(), ErrorKind::UndoConflict);
    assert_eq!(repo.head().await, p.target.oid, "nothing was undone");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("f.txt")).expect("f"),
        "new work after the reset\n"
    );
}

#[tokio::test]
async fn refusals() {
    let (repo, ids) = TestRepo::sample_history().await;
    let head = repo.head().await;
    let p = reset_preview(repo.git(), repo.path(), &head, ResetMode::Soft, &c())
        .await
        .expect("preview");
    assert_eq!(p.refused, Some(ResetRefusal::IsHead));
    repo.run(&["switch", "--quiet", "--detach", &ids["B"]])
        .await;
    let p = reset_preview(repo.git(), repo.path(), &ids["A"], ResetMode::Hard, &c())
        .await
        .expect("preview");
    assert_eq!(p.refused, Some(ResetRefusal::Detached));
    let trash = FakeTrash::new();
    assert!(
        reset(
            repo.git(),
            repo.path(),
            &ids["A"],
            ResetMode::Hard,
            &p.fingerprint,
            &trash,
            &c()
        )
        .await
        .is_err()
    );
    assert_eq!(repo.head().await, ids["B"]);
}
