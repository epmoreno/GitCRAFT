//! Staging and unstaging hunks and lines, checked byte for byte against
//! what ends up in the index.

use gitcraft_core::ops::worktree::{
    SelectionAction, WorktreeArea, apply_selection, worktree_file_diff,
};
use gitcraft_core::parse::diff::{FileDiff, LineKind};
use gitcraft_core::patch::HunkSelection;
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind, Git};

fn git() -> Git {
    gitcraft_core::testing::isolated_git()
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

/// The file as stored in the index.
async fn index(repo: &TestRepo, path: &str) -> String {
    repo.run(&["show", &format!(":{path}")]).await + "\n"
}

/// `git show` drops the final newline in `run`; compare raw bytes instead.
async fn index_bytes(repo: &TestRepo, path: &str) -> Vec<u8> {
    repo.read()
        .args(["cat-file", "blob", &format!(":{path}")])
        .output(&c())
        .await
        .expect("cat-file")
        .stdout
}

async fn diff(repo: &TestRepo, path: &str, area: WorktreeArea) -> FileDiff {
    worktree_file_diff(&git(), repo.path(), path, None, area, false, &c())
        .await
        .expect("diff")
}

async fn apply(
    repo: &TestRepo,
    path: &str,
    area: WorktreeArea,
    selection: Vec<HunkSelection>,
    action: SelectionAction,
) {
    let d = diff(repo, path, area).await;
    apply_selection(
        &git(),
        repo.path(),
        path,
        None,
        area,
        &d.fingerprint,
        &selection,
        action,
        &c(),
    )
    .await
    .expect("apply selection");
}

fn hunk(hunk: u32) -> HunkSelection {
    HunkSelection { hunk, lines: None }
}

/// Indexes of the changed lines in hunk `h` whose content is `text`.
fn line(d: &FileDiff, h: usize, kind: LineKind, text: &str) -> u32 {
    let i = d.hunks[h]
        .lines
        .iter()
        .position(|l| l.kind == kind && l.content == text)
        .unwrap_or_else(|| panic!("{kind:?} {text:?} not in hunk {h}"));
    u32::try_from(i).expect("index")
}

const BASE: &str = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n14\n15\n";

/// Two separate changes: line 2 edited, line 14 removed and a line added.
async fn two_hunks() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", BASE, "base").await;
    repo.write_file(
        "f.txt",
        "1\nTWO\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n15\n16\n",
    );
    repo
}

#[tokio::test]
async fn stages_one_hunk_of_two() {
    let repo = two_hunks().await;
    assert_eq!(
        diff(&repo, "f.txt", WorktreeArea::Unstaged)
            .await
            .hunks
            .len(),
        2
    );
    apply(
        &repo,
        "f.txt",
        WorktreeArea::Unstaged,
        vec![hunk(1)],
        SelectionAction::Stage,
    )
    .await;
    assert_eq!(
        index(&repo, "f.txt").await,
        "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n15\n16\n"
    );
    // The other hunk is still unstaged.
    let rest = diff(&repo, "f.txt", WorktreeArea::Unstaged).await;
    assert_eq!(rest.hunks.len(), 1);
    assert!(rest.hunks[0].lines.iter().any(|l| l.content == "TWO"));
}

#[tokio::test]
async fn stages_single_lines() {
    let repo = two_hunks().await;
    let d = diff(&repo, "f.txt", WorktreeArea::Unstaged).await;
    // Only the added "16", not the removal of 14.
    let added = line(&d, 1, LineKind::Added, "16");
    apply(
        &repo,
        "f.txt",
        WorktreeArea::Unstaged,
        vec![HunkSelection {
            hunk: 1,
            lines: Some(vec![added]),
        }],
        SelectionAction::Stage,
    )
    .await;
    assert_eq!(
        index(&repo, "f.txt").await,
        "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n14\n15\n16\n"
    );

    // Now only the removal of 14.
    let d = diff(&repo, "f.txt", WorktreeArea::Unstaged).await;
    let removed = line(&d, 1, LineKind::Removed, "14");
    apply(
        &repo,
        "f.txt",
        WorktreeArea::Unstaged,
        vec![HunkSelection {
            hunk: 1,
            lines: Some(vec![removed]),
        }],
        SelectionAction::Stage,
    )
    .await;
    assert_eq!(
        index(&repo, "f.txt").await,
        "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n15\n16\n"
    );
}

#[tokio::test]
async fn staging_every_hunk_one_by_one_equals_git_add() {
    let repo = two_hunks().await;
    apply(
        &repo,
        "f.txt",
        WorktreeArea::Unstaged,
        vec![hunk(1)],
        SelectionAction::Stage,
    )
    .await;
    apply(
        &repo,
        "f.txt",
        WorktreeArea::Unstaged,
        vec![hunk(0)],
        SelectionAction::Stage,
    )
    .await;
    let worktree = std::fs::read(repo.path().join("f.txt")).expect("read");
    assert_eq!(index_bytes(&repo, "f.txt").await, worktree);
}

#[tokio::test]
async fn unstages_one_hunk_and_single_lines() {
    let repo = two_hunks().await;
    repo.run(&["add", "f.txt"]).await;

    apply(
        &repo,
        "f.txt",
        WorktreeArea::Staged,
        vec![hunk(0)],
        SelectionAction::Unstage,
    )
    .await;
    assert_eq!(
        index(&repo, "f.txt").await,
        "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n15\n16\n"
    );

    // Unstage only the added "16".
    let d = diff(&repo, "f.txt", WorktreeArea::Staged).await;
    let added = line(&d, 0, LineKind::Added, "16");
    apply(
        &repo,
        "f.txt",
        WorktreeArea::Staged,
        vec![HunkSelection {
            hunk: 0,
            lines: Some(vec![added]),
        }],
        SelectionAction::Unstage,
    )
    .await;
    assert_eq!(
        index(&repo, "f.txt").await,
        "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n15\n"
    );
    // The working tree is untouched throughout.
    let worktree = std::fs::read_to_string(repo.path().join("f.txt")).expect("read");
    assert!(worktree.contains("TWO") && worktree.ends_with("16\n"));
}

#[tokio::test]
async fn stages_part_of_an_untracked_file() {
    let repo = TestRepo::new().await;
    repo.commit_file("keep.txt", "k\n", "base").await;
    repo.write_file("nuevo archivo ñ.txt", "uno\ndos\ntres\n");
    let d = diff(&repo, "nuevo archivo ñ.txt", WorktreeArea::Untracked).await;
    let keep = [
        line(&d, 0, LineKind::Added, "uno"),
        line(&d, 0, LineKind::Added, "tres"),
    ];
    apply(
        &repo,
        "nuevo archivo ñ.txt",
        WorktreeArea::Untracked,
        vec![HunkSelection {
            hunk: 0,
            lines: Some(keep.to_vec()),
        }],
        SelectionAction::Stage,
    )
    .await;
    assert_eq!(index(&repo, "nuevo archivo ñ.txt").await, "uno\ntres\n");
}

#[tokio::test]
async fn handles_files_without_a_final_newline() {
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "a\nb", "no final newline").await;
    repo.write_file("f.txt", "a\nb\nc");
    apply(
        &repo,
        "f.txt",
        WorktreeArea::Unstaged,
        vec![hunk(0)],
        SelectionAction::Stage,
    )
    .await;
    assert_eq!(index_bytes(&repo, "f.txt").await, b"a\nb\nc");

    // And back.
    apply(
        &repo,
        "f.txt",
        WorktreeArea::Staged,
        vec![hunk(0)],
        SelectionAction::Unstage,
    )
    .await;
    assert_eq!(index_bytes(&repo, "f.txt").await, b"a\nb");
}

#[tokio::test]
async fn refuses_a_stale_selection_without_touching_the_index() {
    let repo = two_hunks().await;
    let seen = diff(&repo, "f.txt", WorktreeArea::Unstaged).await;
    // The file changes after the user looked at it.
    repo.write_file("f.txt", "completely\ndifferent\n");
    let err = apply_selection(
        &git(),
        repo.path(),
        "f.txt",
        None,
        WorktreeArea::Unstaged,
        &seen.fingerprint,
        &[hunk(0)],
        SelectionAction::Stage,
        &c(),
    )
    .await
    .expect_err("stale");
    assert_eq!(err.kind(), ErrorKind::StaleDiff);
    assert_eq!(index(&repo, "f.txt").await, BASE);
}

#[tokio::test]
async fn rejects_impossible_combinations() {
    let repo = two_hunks().await;
    let d = diff(&repo, "f.txt", WorktreeArea::Unstaged).await;
    let unstage_unstaged = apply_selection(
        &git(),
        repo.path(),
        "f.txt",
        None,
        WorktreeArea::Unstaged,
        &d.fingerprint,
        &[hunk(0)],
        SelectionAction::Unstage,
        &c(),
    )
    .await;
    assert!(unstage_unstaged.is_err());
    assert_eq!(index(&repo, "f.txt").await, BASE);
}

/// CRLF base with lines 1–15.
fn crlf(lines: &[&str]) -> String {
    lines.iter().map(|l| format!("{l}\r\n")).collect()
}

const NUMBERS: [&str; 15] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15",
];

/// P.8: a file with CRLF line endings stored as such (core.autocrlf=false):
/// staging one hunk puts exactly that change in the index, CRs included,
/// and leaves the working tree alone.
#[tokio::test]
async fn stages_one_hunk_of_a_crlf_file_byte_for_byte() {
    let repo = TestRepo::new().await;
    repo.run(&["config", "core.autocrlf", "false"]).await;
    repo.commit_file("f.txt", &crlf(&NUMBERS), "base").await;
    let mut edited = NUMBERS.to_vec();
    edited[1] = "TWO";
    edited[13] = "FOURTEEN";
    repo.write_file("f.txt", &crlf(&edited));
    apply(
        &repo,
        "f.txt",
        WorktreeArea::Unstaged,
        vec![hunk(1)],
        SelectionAction::Stage,
    )
    .await;
    let mut expected = NUMBERS.to_vec();
    expected[13] = "FOURTEEN";
    assert_eq!(
        index_bytes(&repo, "f.txt").await,
        crlf(&expected).into_bytes()
    );
    assert_eq!(
        std::fs::read(repo.path().join("f.txt")).expect("read"),
        crlf(&edited).into_bytes()
    );
}

/// P.8: with core.autocrlf=true (Git for Windows' default) the index keeps
/// LF while the working tree has CRLF; staging one hunk keeps it that way.
#[tokio::test]
async fn stages_one_hunk_with_autocrlf_keeping_lf_in_the_index() {
    let repo = TestRepo::new().await;
    repo.run(&["config", "core.autocrlf", "true"]).await;
    repo.write_file("f.txt", &crlf(&NUMBERS));
    repo.run(&["add", "f.txt"]).await;
    repo.run(&["commit", "-q", "-m", "base"]).await;
    let mut edited = NUMBERS.to_vec();
    edited[1] = "TWO";
    edited[13] = "FOURTEEN";
    repo.write_file("f.txt", &crlf(&edited));
    apply(
        &repo,
        "f.txt",
        WorktreeArea::Unstaged,
        vec![hunk(0)],
        SelectionAction::Stage,
    )
    .await;
    let mut expected = NUMBERS.to_vec();
    expected[1] = "TWO";
    let lf: String = expected.iter().map(|l| format!("{l}\n")).collect();
    assert_eq!(index_bytes(&repo, "f.txt").await, lf.into_bytes());
    let rest = diff(&repo, "f.txt", WorktreeArea::Unstaged).await;
    assert_eq!(rest.hunks.len(), 1);
    assert!(rest.hunks[0].lines.iter().any(|l| l.content == "FOURTEEN"));
}
