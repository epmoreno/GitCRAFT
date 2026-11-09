//! Discarding changes and undoing it, checked byte for byte.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gitcraft_core::ops::discard::{
    DiscardTarget, SystemTrash, Trash, discard, preview_discard, undo_discard,
};
#[cfg(unix)]
use gitcraft_core::ops::discard::{preview_discard_with, undo_discard_with};
use gitcraft_core::ops::worktree::{WorktreeArea, worktree_file_diff};
use gitcraft_core::parse::diff::LineKind;
use gitcraft_core::patch::HunkSelection;
#[cfg(unix)]
use gitcraft_core::platform::FileSupport;
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind, Git};

fn git() -> Git {
    gitcraft_core::testing::isolated_git()
}

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

fn files(paths: &[&str]) -> DiscardTarget {
    DiscardTarget::Files {
        paths: paths.iter().map(|p| p.to_string()).collect(),
    }
}

/// Every file of the working tree (outside .git) with its bytes and mode.
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>, u32, Option<PathBuf>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>, u32, Option<PathBuf>)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).expect("dir").flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            let meta = std::fs::symlink_metadata(&path).expect("meta");
            let rel = path
                .strip_prefix(root)
                .expect("rel")
                .to_string_lossy()
                .into_owned();
            if meta.file_type().is_symlink() {
                out.push((
                    rel,
                    Vec::new(),
                    0,
                    Some(std::fs::read_link(&path).expect("link")),
                ));
            } else if meta.is_dir() {
                walk(root, &path, out);
            } else {
                out.push((
                    rel,
                    std::fs::read(&path).expect("read"),
                    gitcraft_core::platform::file_mode(&meta),
                    None,
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

async fn index_tree(repo: &TestRepo) -> String {
    repo.run(&["write-tree"]).await
}

/// `name` as a symbolic link to `target` where the system creates them; a
/// file holding the target elsewhere (what git checks out without symlink
/// support).
fn make_link(repo: &TestRepo, target: &str, name: &str) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, repo.path().join(name)).expect("symlink");
    #[cfg(not(unix))]
    repo.write_file(name, target);
}

/// Tracked files with awkward content, then modified in the working tree.
async fn awkward_repo() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.write_file("crlf.txt", "uno\r\ndos\r\n");
    repo.write_file("no-newline.txt", "a\nb");
    std::fs::write(repo.path().join("data.bin"), [0u8, 159, 146, 150, 0, 255]).expect("bin");
    repo.write_file("script.sh", "#!/bin/sh\necho hola\n");
    repo.write_file("ñandú/acentos.md", "café\n");
    repo.write_file("gone.txt", "will be deleted\n");
    make_link(&repo, "crlf.txt", "link");
    repo.commit_all("base").await;
    // Line endings converted on checkout must not affect the backup.
    repo.run(&["config", "core.autocrlf", "true"]).await;

    repo.write_file("crlf.txt", "uno\r\nDOS\r\ntres\r\n");
    repo.write_file("no-newline.txt", "a\nb\nc");
    std::fs::write(repo.path().join("data.bin"), [1u8, 2, 3, 0, 0, 7]).expect("bin");
    repo.write_file("script.sh", "#!/bin/sh\necho adiós\n");
    gitcraft_core::platform::set_file_mode(&repo.path().join("script.sh"), 0o755).expect("chmod");
    repo.write_file("ñandú/acentos.md", "café con leche\n");
    std::fs::remove_file(repo.path().join("gone.txt")).expect("rm");
    std::fs::remove_file(repo.path().join("link")).expect("rm link");
    make_link(&repo, "no-newline.txt", "link");
    repo
}

/// Where symbolic links cannot be created, the dialog warns before the
/// discard, and "Undo" refuses without touching anything (the stash copy
/// stays).
#[cfg(unix)]
#[tokio::test]
async fn without_symlink_support_undo_refuses_and_changes_nothing() {
    let repo = awkward_repo().await;
    let trash = FakeTrash::new();
    let none = FileSupport { symlinks: false };
    let paths = ["link", "script.sh"];

    let preview = preview_discard_with(&git(), repo.path(), &files(&paths), none, &c())
        .await
        .expect("preview");
    let flagged: Vec<_> = preview
        .iter()
        .map(|f| (f.path.as_str(), f.undo_unsupported))
        .collect();
    assert_eq!(flagged, [("link", true), ("script.sh", false)]);
    let here = preview_discard(&git(), repo.path(), &files(&paths), &c())
        .await
        .expect("preview here");
    assert!(
        here.iter().all(|f| !f.undo_unsupported),
        "Unix restores links"
    );

    let (_, record) = discard(&git(), repo.path(), &files(&paths), &trash, &c())
        .await
        .expect("discard");
    assert!(record.stash.is_some(), "the stash copy is kept");
    let after_discard = snapshot(repo.path());

    let err = undo_discard_with(&git(), repo.path(), &record, none, &c())
        .await
        .expect_err("cannot recreate the link");
    assert_eq!(err.kind(), ErrorKind::UndoUnsupported);
    assert_eq!(snapshot(repo.path()), after_discard, "nothing was touched");
    assert_eq!(
        repo.run(&["stash", "list", "--format=%gs"]).await,
        "GitCRAFT: descarte de link, script.sh"
    );
}

#[tokio::test]
async fn discarding_and_undoing_tracked_files_is_byte_exact() {
    let repo = awkward_repo().await;
    let trash = FakeTrash::new();
    let paths = [
        "crlf.txt",
        "no-newline.txt",
        "data.bin",
        "script.sh",
        "ñandú/acentos.md",
        "gone.txt",
        "link",
    ];
    let before = snapshot(repo.path());
    let index_before = index_tree(&repo).await;

    let (discarded, record) = discard(&git(), repo.path(), &files(&paths), &trash, &c())
        .await
        .expect("discard");
    assert_eq!(discarded.len(), paths.len());
    // Back to the committed version, as git checks it out (with
    // core.autocrlf=true that means CRLF line endings).
    assert_eq!(
        std::fs::read(repo.path().join("no-newline.txt")).expect("read"),
        b"a\r\nb"
    );
    assert!(repo.path().join("gone.txt").exists(), "deletion discarded");
    assert_eq!(
        index_tree(&repo).await,
        index_before,
        "the index is untouched"
    );

    undo_discard(&git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(snapshot(repo.path()), before, "identical byte for byte");
    assert!(!repo.path().join("gone.txt").exists());
    assert_eq!(index_tree(&repo).await, index_before);
}

#[tokio::test]
async fn keeps_a_stash_backup_until_undone() {
    let repo = awkward_repo().await;
    let trash = FakeTrash::new();
    let (_, record) = discard(
        &git(),
        repo.path(),
        &files(&["ñandú/acentos.md"]),
        &trash,
        &c(),
    )
    .await
    .expect("discard");
    let list = repo.run(&["stash", "list", "--format=%gs"]).await;
    assert_eq!(list, "GitCRAFT: descarte de ñandú/acentos.md");
    // The stash holds the discarded working tree content.
    assert_eq!(
        repo.run(&["show", "stash@{0}:ñandú/acentos.md"]).await,
        "café con leche"
    );

    undo_discard(&git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(
        repo.run(&["stash", "list"]).await,
        "",
        "redundant backup dropped"
    );
}

#[tokio::test]
async fn untracked_files_and_directories_go_to_the_trash_and_come_back() {
    let repo = TestRepo::new().await;
    repo.commit_file("keep.txt", "k\n", "base").await;
    repo.write_file("loose.txt", "suelto\nsin final");
    repo.write_file("notes/a.md", "a\n");
    repo.write_file("notes/deep/b.md", "b\n");
    let trash = FakeTrash::new();
    let before = snapshot(repo.path());

    let target = files(&["loose.txt", "notes/"]);
    let preview = preview_discard(&git(), repo.path(), &target, &c())
        .await
        .expect("preview");
    assert_eq!(preview[0].lines, Some(2));
    assert_eq!(preview[1].file_count, Some(2));

    let (_, record) = discard(&git(), repo.path(), &target, &trash, &c())
        .await
        .expect("discard");
    assert!(!repo.path().join("loose.txt").exists());
    assert!(!repo.path().join("notes").exists());
    assert_eq!(
        std::fs::read_dir(trash.dir.path()).expect("trash").count(),
        2
    );
    assert_eq!(
        repo.run(&["stash", "list"]).await,
        "",
        "no stash for untracked only"
    );

    undo_discard(&git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(snapshot(repo.path()), before);
}

#[tokio::test]
async fn discarding_a_hunk_or_lines_is_just_as_recoverable() {
    let repo = TestRepo::new().await;
    let base: String = (1..=15).map(|i| format!("{i}\n")).collect();
    repo.commit_file("f.txt", &base, "base").await;
    repo.write_file(
        "f.txt",
        "1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n15\n16\n",
    );
    let trash = FakeTrash::new();
    let before = snapshot(repo.path());

    // Discard the second hunk only.
    let diff = worktree_file_diff(
        &git(),
        repo.path(),
        "f.txt",
        None,
        WorktreeArea::Unstaged,
        false,
        &c(),
    )
    .await
    .expect("diff");
    let target = DiscardTarget::Selection {
        path: "f.txt".into(),
        fingerprint: diff.fingerprint.clone(),
        selection: vec![HunkSelection {
            hunk: 1,
            lines: None,
        }],
    };
    let preview = preview_discard(&git(), repo.path(), &target, &c())
        .await
        .expect("preview");
    assert_eq!((preview[0].added, preview[0].removed), (Some(1), Some(1)));
    let (_, record) = discard(&git(), repo.path(), &target, &trash, &c())
        .await
        .expect("discard hunk");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("f.txt")).expect("read"),
        "1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n14\n15\n"
    );
    undo_discard(&git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(snapshot(repo.path()), before);

    // Discard a single added line ("16").
    let diff = worktree_file_diff(
        &git(),
        repo.path(),
        "f.txt",
        None,
        WorktreeArea::Unstaged,
        false,
        &c(),
    )
    .await
    .expect("diff");
    let line = diff.hunks[1]
        .lines
        .iter()
        .position(|l| l.kind == LineKind::Added && l.content == "16")
        .expect("line") as u32;
    let target = DiscardTarget::Selection {
        path: "f.txt".into(),
        fingerprint: diff.fingerprint,
        selection: vec![HunkSelection {
            hunk: 1,
            lines: Some(vec![line]),
        }],
    };
    let (_, record) = discard(&git(), repo.path(), &target, &trash, &c())
        .await
        .expect("discard line");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("f.txt")).expect("read"),
        "1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n15\n"
    );
    undo_discard(&git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(snapshot(repo.path()), before);
}

/// After the undo window: a file with staged and unstaged changes, part of
/// it discarded, is recovered from the stash backup with
/// `git restore --source=<stash> --worktree -- <path>` (`stash apply` refuses
/// here because the file still has other changes).
#[tokio::test]
async fn a_partial_discard_is_recoverable_from_the_stash_later() {
    let repo = TestRepo::new().await;
    let base: String = (1..=15).map(|i| format!("{i}\n")).collect();
    repo.commit_file("f.txt", &base, "base").await;
    // A staged change at the top…
    repo.write_file("f.txt", &base.replacen("2\n", "staged\n", 1));
    repo.run(&["add", "f.txt"]).await;
    // …and unstaged changes in two other places.
    repo.write_file(
        "f.txt",
        "1\nstaged\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\nDOCE\n14\n15\nnuevo\n",
    );
    let before = std::fs::read(repo.path().join("f.txt")).expect("read");
    let index_before = index_tree(&repo).await;
    let trash = FakeTrash::new();

    let diff = worktree_file_diff(
        &git(),
        repo.path(),
        "f.txt",
        None,
        WorktreeArea::Unstaged,
        false,
        &c(),
    )
    .await
    .expect("diff");
    let line = diff.hunks[0]
        .lines
        .iter()
        .position(|l| l.kind == LineKind::Added && l.content == "nuevo")
        .expect("line") as u32;
    let target = DiscardTarget::Selection {
        path: "f.txt".into(),
        fingerprint: diff.fingerprint,
        selection: vec![HunkSelection {
            hunk: 0,
            lines: Some(vec![line]),
        }],
    };
    discard(&git(), repo.path(), &target, &trash, &c())
        .await
        .expect("discard line");
    let after = std::fs::read(repo.path().join("f.txt")).expect("read");
    assert_ne!(after, before);
    // The undo window passes; the file is still modified and staged.

    let apply = repo
        .write()
        .args(["stash", "apply", "stash@{0}"])
        .output(&c())
        .await;
    assert!(apply.is_err(), "stash apply refuses over local changes");
    assert_eq!(
        std::fs::read(repo.path().join("f.txt")).expect("read"),
        after
    );

    repo.run(&["restore", "--source=stash@{0}", "--worktree", "--", "f.txt"])
        .await;
    assert_eq!(
        std::fs::read(repo.path().join("f.txt")).expect("read"),
        before,
        "identical byte for byte"
    );
    assert_eq!(
        index_tree(&repo).await,
        index_before,
        "the index is untouched"
    );
    assert_eq!(
        repo.run(&["diff", "--cached", "--name-only"]).await,
        "f.txt",
        "the staged change is still staged"
    );
}

#[tokio::test]
async fn staged_changes_are_never_discarded() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.write_file("f.txt", "staged\n");
    repo.run(&["add", "f.txt"]).await;
    repo.write_file("f.txt", "staged\nand more\n");
    let trash = FakeTrash::new();
    discard(&git(), repo.path(), &files(&["f.txt"]), &trash, &c())
        .await
        .expect("discard");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("f.txt")).expect("read"),
        "staged\n",
        "back to the staged version, not to HEAD"
    );
}

#[tokio::test]
async fn undo_never_overwrites_newer_work() {
    let repo = awkward_repo().await;
    let trash = FakeTrash::new();
    let (_, record) = discard(
        &git(),
        repo.path(),
        &files(&["no-newline.txt"]),
        &trash,
        &c(),
    )
    .await
    .expect("discard");
    repo.write_file("no-newline.txt", "edited after the discard\n");
    let err = undo_discard(&git(), repo.path(), &record, &c())
        .await
        .expect_err("conflict");
    assert_eq!(err.kind(), ErrorKind::UndoConflict);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("no-newline.txt")).expect("read"),
        "edited after the discard\n"
    );
    // The stash backup is still there.
    assert!(
        repo.run(&["stash", "list"])
            .await
            .contains("GitCRAFT: descarte de")
    );
}

#[tokio::test]
async fn works_before_the_first_commit() {
    let repo = TestRepo::new().await;
    repo.write_file("new.txt", "staged\n");
    repo.run(&["add", "new.txt"]).await;
    repo.write_file("new.txt", "staged\nunstaged\n");
    let trash = FakeTrash::new();
    let before = snapshot(repo.path());
    let (_, record) = discard(&git(), repo.path(), &files(&["new.txt"]), &trash, &c())
        .await
        .expect("discard");
    assert_eq!(
        record.stash, None,
        "no stash without a commit; blobs suffice"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("new.txt")).expect("read"),
        "staged\n"
    );
    undo_discard(&git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(snapshot(repo.path()), before);
}

#[tokio::test]
async fn refuses_stale_selections_and_paths_without_changes() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "a\nb\n", "base").await;
    repo.write_file("f.txt", "a\nB\n");
    let trash = FakeTrash::new();
    let stale = DiscardTarget::Selection {
        path: "f.txt".into(),
        fingerprint: "not-the-current-diff".into(),
        selection: vec![HunkSelection {
            hunk: 0,
            lines: None,
        }],
    };
    let err = discard(&git(), repo.path(), &stale, &trash, &c())
        .await
        .expect_err("stale");
    assert_eq!(err.kind(), ErrorKind::StaleDiff);
    assert!(
        discard(&git(), repo.path(), &files(&["clean.txt"]), &trash, &c())
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("f.txt")).expect("read"),
        "a\nB\n",
        "nothing was touched"
    );
}

/// Uses the real macOS Trash: run by hand with
/// `cargo test -p gitcraft-core --test discard -- --ignored`.
#[tokio::test]
#[ignore = "touches the real Trash"]
async fn the_system_trash_round_trips() {
    let repo = TestRepo::new().await;
    repo.commit_file("keep.txt", "k\n", "base").await;
    repo.write_file("gitcraft-trash-test.txt", "x\n");
    let before = snapshot(repo.path());
    let (_, record) = discard(
        &git(),
        repo.path(),
        &files(&["gitcraft-trash-test.txt"]),
        &SystemTrash,
        &c(),
    )
    .await
    .expect("discard");
    assert!(!repo.path().join("gitcraft-trash-test.txt").exists());
    undo_discard(&git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(snapshot(repo.path()), before);
}

/// A Trash that takes `ok` files and then refuses, as a volume without a
/// Trash would.
struct FailingTrash {
    inner: FakeTrash,
    ok: Mutex<u32>,
}

impl Trash for FailingTrash {
    fn trash(&self, path: &Path) -> io::Result<PathBuf> {
        let mut ok = self.ok.lock().expect("lock");
        if *ok == 0 {
            return Err(io::Error::other("there is no Trash on this volume"));
        }
        *ok -= 1;
        self.inner.trash(path)
    }
}

/// P.5: if any untracked file cannot go to the Trash, the discard stops with
/// a clear error and nothing has changed: the files already trashed come
/// back, tracked changes stay and no stash entry is made. Nothing is ever
/// deleted instead.
#[tokio::test]
async fn when_a_file_cannot_go_to_the_trash_nothing_changes() {
    let repo = TestRepo::new().await;
    repo.commit_file("tracked.txt", "base\n", "base").await;
    repo.write_file("tracked.txt", "edited\n");
    repo.write_file("a-new.txt", "first\n");
    repo.write_file("b-new.txt", "second\n");
    let before = snapshot(repo.path());
    let trash = FailingTrash {
        inner: FakeTrash::new(),
        ok: Mutex::new(1),
    };
    let err = discard(
        &git(),
        repo.path(),
        &files(&["tracked.txt", "a-new.txt", "b-new.txt"]),
        &trash,
        &c(),
    )
    .await
    .expect_err("the second file has no Trash");
    assert_eq!(err.kind(), ErrorKind::TrashFailed, "{err}");
    assert!(err.to_string().contains("b-new.txt"), "{err}");
    assert_eq!(snapshot(repo.path()), before, "nothing changed");
    assert!(repo.run(&["stash", "list"]).await.trim().is_empty());
}

#[cfg(unix)]
mod freedesktop {
    use super::*;
    use gitcraft_core::freedesktop_trash::FreedesktopTrash;

    /// The real freedesktop Trash code, with its home Trash in a temporary
    /// folder (on the same volume as the test repositories).
    fn trash() -> (tempfile::TempDir, PathBuf, FreedesktopTrash) {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path().join("Trash");
        let uid = std::fs::metadata(dir.path())
            .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
            .expect("uid");
        let trash = FreedesktopTrash::with_home_trash(home.clone(), uid);
        (dir, home, trash)
    }

    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    /// P.5: discard → Trash → Undo gives back every file byte for byte,
    /// folders and odd names included, and the Trash records each one as
    /// the spec says.
    #[tokio::test]
    async fn discard_to_the_trash_and_undo_byte_for_byte() {
        let repo = TestRepo::new().await;
        repo.commit_file("keep.txt", "k\n", "base").await;
        let all_bytes: Vec<u8> = (0..=255).collect();
        std::fs::write(repo.path().join("bytes.bin"), &all_bytes).expect("bin");
        repo.write_file("año nuevo & más.txt", "uno\r\ndos");
        repo.write_file("build/out/a.o", "a");
        repo.write_file("build/b.o", "b");
        let before = snapshot(repo.path());
        let (_dir, home, trash) = trash();

        let (_, record) = discard(
            &git(),
            repo.path(),
            &files(&["bytes.bin", "año nuevo & más.txt", "build/"]),
            &trash,
            &c(),
        )
        .await
        .expect("discard");
        let left: Vec<String> = snapshot(repo.path()).into_iter().map(|f| f.0).collect();
        assert_eq!(left, ["keep.txt"]);
        assert_eq!(
            listing(&home.join("files")),
            ["año nuevo & más.txt", "build", "bytes.bin"]
        );
        let info =
            std::fs::read_to_string(home.join("info/año nuevo & más.txt.trashinfo")).expect("info");
        assert!(info.starts_with("[Trash Info]\nPath=/"), "{info}");
        assert!(
            info.contains("/a%C3%B1o%20nuevo%20%26%20m%C3%A1s.txt\nDeletionDate="),
            "{info}"
        );

        undo_discard(&git(), repo.path(), &record, &c())
            .await
            .expect("undo");
        assert_eq!(snapshot(repo.path()), before, "byte for byte");
        assert!(listing(&home.join("files")).is_empty());
        #[cfg(not(target_os = "macos"))]
        assert!(
            listing(&home.join("info")).is_empty(),
            "the info files go with the undo"
        );
    }

    /// Two discards of files with the same name keep both in the Trash, and
    /// each undo brings back its own.
    #[tokio::test]
    async fn same_names_do_not_collide() {
        let repo = TestRepo::new().await;
        repo.commit_file("keep.txt", "k\n", "base").await;
        let (_dir, home, trash) = trash();

        repo.write_file("notes.txt", "first\n");
        let (_, first) = discard(&git(), repo.path(), &files(&["notes.txt"]), &trash, &c())
            .await
            .expect("first");
        repo.write_file("notes.txt", "second\n");
        let (_, second) = discard(&git(), repo.path(), &files(&["notes.txt"]), &trash, &c())
            .await
            .expect("second");
        assert_eq!(listing(&home.join("files")), ["notes.txt", "notes.txt.2"]);
        assert_eq!(
            listing(&home.join("info")),
            ["notes.txt.2.trashinfo", "notes.txt.trashinfo"]
        );

        undo_discard(&git(), repo.path(), &second, &c())
            .await
            .expect("undo second");
        assert_eq!(
            std::fs::read_to_string(repo.path().join("notes.txt")).expect("read"),
            "second\n"
        );
        std::fs::remove_file(repo.path().join("notes.txt")).expect("rm");
        undo_discard(&git(), repo.path(), &first, &c())
            .await
            .expect("undo first");
        assert_eq!(
            std::fs::read_to_string(repo.path().join("notes.txt")).expect("read"),
            "first\n"
        );
    }
}

/// Every entry of the Recycle Bin folders this user can read on `drive`
/// (`desktop.ini`, which Windows creates on first use, aside).
#[cfg(windows)]
fn recycle_bin(drive: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    for user in std::fs::read_dir(drive.join("$Recycle.Bin"))
        .into_iter()
        .flatten()
        .flatten()
    {
        for entry in std::fs::read_dir(user.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            if entry.file_name() != "desktop.ini" {
                entries.push(entry.path());
            }
        }
    }
    entries.sort();
    entries
}

/// P.5: discard → Recycle Bin → Undo with the real Recycle Bin: every file
/// comes back byte for byte and the Recycle Bin is left as it was. Always
/// runs in CI; elsewhere only when asked:
/// `cargo test -p gitcraft-core --test discard -- --ignored`.
#[cfg(windows)]
#[tokio::test]
#[cfg_attr(not(ci), ignore = "touches the real Recycle Bin")]
async fn the_recycle_bin_round_trips_and_is_left_as_it_was() {
    let repo = TestRepo::new().await;
    repo.commit_file("keep.txt", "k\n", "base").await;
    let all_bytes: Vec<u8> = (0..=255).collect();
    std::fs::write(repo.path().join("bytes.bin"), &all_bytes).expect("bin");
    repo.write_file("año nuevo & más.txt", "uno\r\ndos");
    repo.write_file("build/out/a.o", "a");
    repo.write_file("build/b.o", "b");
    let before = snapshot(repo.path());
    let drive: PathBuf = repo.path().components().take(2).collect();
    let bin_before = recycle_bin(&drive);

    let (_, record) = discard(
        &git(),
        repo.path(),
        &files(&["bytes.bin", "año nuevo & más.txt", "build/"]),
        &SystemTrash,
        &c(),
    )
    .await
    .expect("discard");
    let left: Vec<String> = snapshot(repo.path()).into_iter().map(|f| f.0).collect();
    assert_eq!(left, ["keep.txt"]);
    assert_eq!(
        recycle_bin(&drive).len(),
        bin_before.len() + 6,
        "three items, each with its $R and $I entries"
    );

    undo_discard(&git(), repo.path(), &record, &c())
        .await
        .expect("undo");
    assert_eq!(snapshot(repo.path()), before, "byte for byte");
    assert_eq!(recycle_bin(&drive), bin_before, "the Recycle Bin as it was");
}
