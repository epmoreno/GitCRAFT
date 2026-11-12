//! Reading files in conflict from real merges: classification, blocks and
//! the base of each block. Nothing is written to the repository.

use gitcraft_core::ops::conflict::{ConflictContent, ConflictFile, WholeFileReason, conflict_file};
use gitcraft_core::parse::conflict::{ConflictBlock, Segment, render_segments};
use gitcraft_core::parse::status::ConflictKind;
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind};

fn c() -> CancellationToken {
    CancellationToken::new()
}

const BASE: &str =
    "fn main() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n\n// one\n// two\n";
const MAIN: &str =
    "fn main() {\n    let a = 100;\n    let b = 2;\n    let c = 3;\n}\n\n// one\n// two (main)\n";
const FEATURE: &str =
    "fn main() {\n    let a = 10;\n    let b = 2;\n    let c = 3;\n}\n\n// one\n// two (feature)\n";

/// `path` is `base` in the first commit, `feature` on branch `feature` and
/// `main` on `main`; then `feature` is merged into `main` and stops.
async fn merge_conflict(path: &str, base: &[u8], main: &[u8], feature: &[u8]) -> TestRepo {
    let repo = TestRepo::new().await;
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

fn write(repo: &TestRepo, path: &str, bytes: &[u8]) {
    let full = repo.path().join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("dirs");
    }
    std::fs::write(full, bytes).expect("write");
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

async fn read(repo: &TestRepo, path: &str) -> ConflictFile {
    conflict_file(repo.git(), repo.path(), path, &c())
        .await
        .expect("conflict_file")
}

fn blocks(file: &ConflictFile) -> (Vec<&ConflictBlock>, &[Segment]) {
    let ConflictContent::Blocks { segments, .. } = &file.content else {
        panic!("expected blocks, got {:?}", file.content);
    };
    let blocks = segments
        .iter()
        .filter_map(|s| match s {
            Segment::Conflict { block } => Some(block),
            Segment::Common { .. } => None,
        })
        .collect();
    (blocks, segments)
}

fn reason(file: &ConflictFile) -> WholeFileReason {
    match file.content {
        ConflictContent::WholeFile { reason } => reason,
        ConflictContent::Blocks { .. } => panic!("expected a whole-file conflict"),
    }
}

#[tokio::test]
async fn text_conflict_with_computed_bases() {
    let repo = merge_conflict("f.rs", BASE.as_bytes(), MAIN.as_bytes(), FEATURE.as_bytes()).await;
    let file = read(&repo, "f.rs").await;
    assert_eq!(file.kind, ConflictKind::BothModified);
    assert!(file.base.is_some() && file.ours.is_some() && file.theirs.is_some());
    assert!(file.fingerprint.is_some());

    let (blocks, segments) = blocks(&file);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].ours, "    let a = 100;\n");
    assert_eq!(blocks[0].theirs, "    let a = 10;\n");
    assert_eq!(blocks[0].base.as_deref(), Some("    let a = 1;\n"));
    assert_eq!(blocks[1].base.as_deref(), Some("// two\n"));
    assert!(blocks.iter().all(|b| b.base_marker.is_none()));

    // The computed bases are not part of the file.
    let on_disk = std::fs::read_to_string(repo.path().join("f.rs")).expect("read");
    assert_eq!(render_segments(segments), on_disk);
}

#[tokio::test]
async fn diff3_style_takes_the_base_from_the_markers() {
    let repo = TestRepo::new().await;
    repo.run(&["config", "merge.conflictStyle", "diff3"]).await;
    write(&repo, "f.rs", BASE.as_bytes());
    repo.commit_all("base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    write(&repo, "f.rs", FEATURE.as_bytes());
    repo.commit_all("feature").await;
    repo.checkout("main").await;
    write(&repo, "f.rs", MAIN.as_bytes());
    repo.commit_all("main").await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;

    let file = read(&repo, "f.rs").await;
    let (blocks, segments) = blocks(&file);
    assert!(blocks.iter().all(|b| b.base_marker.is_some()));
    assert_eq!(blocks[0].base.as_deref(), Some("    let a = 1;\n"));
    let on_disk = std::fs::read_to_string(repo.path().join("f.rs")).expect("read");
    assert_eq!(render_segments(segments), on_disk);
}

#[tokio::test]
async fn an_edited_block_has_no_base() {
    let repo = merge_conflict("f.rs", BASE.as_bytes(), MAIN.as_bytes(), FEATURE.as_bytes()).await;
    let path = repo.path().join("f.rs");
    let edited = std::fs::read_to_string(&path)
        .expect("read")
        .replace("    let a = 100;\n", "    let a = 1000;\n");
    std::fs::write(&path, edited).expect("write");

    let file = read(&repo, "f.rs").await;
    let (blocks, _) = blocks(&file);
    assert_eq!(blocks[0].base, None);
    assert_eq!(blocks[1].base.as_deref(), Some("// two\n"));
}

#[tokio::test]
async fn crlf_files_keep_their_bytes() {
    let crlf = |s: &str| s.replace('\n', "\r\n").into_bytes();
    let repo = merge_conflict("f.rs", &crlf(BASE), &crlf(MAIN), &crlf(FEATURE)).await;
    let file = read(&repo, "f.rs").await;
    let (blocks, segments) = blocks(&file);
    assert_eq!(blocks[0].ours, "    let a = 100;\r\n");
    assert_eq!(blocks[0].base.as_deref(), Some("    let a = 1;\r\n"));
    let on_disk = std::fs::read(repo.path().join("f.rs")).expect("read");
    assert_eq!(render_segments(segments).into_bytes(), on_disk);
}

/// With `core.autocrlf=true` the index has LF and the working tree CRLF:
/// bases are computed from the versions as checked out.
#[tokio::test]
async fn autocrlf_bases_match_the_working_tree() {
    let repo = TestRepo::new().await;
    repo.run(&["config", "core.autocrlf", "true"]).await;
    write(&repo, "f.rs", BASE.as_bytes());
    repo.commit_all("base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    write(&repo, "f.rs", FEATURE.as_bytes());
    repo.commit_all("feature").await;
    repo.checkout("main").await;
    write(&repo, "f.rs", MAIN.as_bytes());
    repo.commit_all("main").await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;

    let file = read(&repo, "f.rs").await;
    let (blocks, segments) = blocks(&file);
    assert_eq!(blocks[0].ours, "    let a = 100;\r\n");
    assert_eq!(blocks[0].base.as_deref(), Some("    let a = 1;\r\n"));
    let on_disk = std::fs::read(repo.path().join("f.rs")).expect("read");
    assert_eq!(render_segments(segments).into_bytes(), on_disk);
}

#[tokio::test]
async fn binary_files_are_resolved_whole() {
    let repo = merge_conflict("img.bin", b"\0base\0", b"\0main\0", b"\0feature\0").await;
    let file = read(&repo, "img.bin").await;
    assert_eq!(file.kind, ConflictKind::BothModified);
    assert_eq!(reason(&file), WholeFileReason::Binary);
}

#[tokio::test]
async fn deleted_on_one_side() {
    // Deleted on main, modified on feature.
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    repo.commit_file("f.txt", "feature\n", "feature").await;
    repo.checkout("main").await;
    repo.run(&["rm", "--quiet", "--", "f.txt"]).await;
    repo.commit_all("delete").await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;

    let file = read(&repo, "f.txt").await;
    assert_eq!(file.kind, ConflictKind::DeletedByUs);
    assert!(file.ours.is_none() && file.theirs.is_some());
    assert_eq!(reason(&file), WholeFileReason::OneSideMissing);

    // Modified on main, deleted on feature.
    let repo = TestRepo::new().await;
    repo.commit_file("f.txt", "base\n", "base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    repo.run(&["rm", "--quiet", "--", "f.txt"]).await;
    repo.commit_all("delete").await;
    repo.checkout("main").await;
    repo.commit_file("f.txt", "main\n", "main").await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;

    let file = read(&repo, "f.txt").await;
    assert_eq!(file.kind, ConflictKind::DeletedByThem);
    assert_eq!(reason(&file), WholeFileReason::OneSideMissing);
}

#[cfg(unix)]
#[tokio::test]
async fn symbolic_links_are_resolved_whole() {
    let repo = TestRepo::new().await;
    let link = repo.path().join("link");
    std::os::unix::fs::symlink("base-target", &link).expect("symlink");
    repo.commit_all("base").await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    std::fs::remove_file(&link).expect("rm");
    std::os::unix::fs::symlink("feature-target", &link).expect("symlink");
    repo.commit_all("feature").await;
    repo.checkout("main").await;
    std::fs::remove_file(&link).expect("rm");
    std::os::unix::fs::symlink("main-target", &link).expect("symlink");
    repo.commit_all("main").await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;

    let file = read(&repo, "link").await;
    assert_eq!(reason(&file), WholeFileReason::Symlink);
}

#[tokio::test]
async fn broken_markers_are_reported_with_their_line() {
    let repo = merge_conflict("f.rs", BASE.as_bytes(), MAIN.as_bytes(), FEATURE.as_bytes()).await;
    let path = repo.path().join("f.rs");
    let text = std::fs::read_to_string(&path).expect("read");
    // The user removed the end of the first block by hand.
    let broken = text.replacen(">>>>>>> feature\n", "", 1);
    std::fs::write(&path, broken).expect("write");

    // The first block now runs into the opening marker of the second one.
    let file = read(&repo, "f.rs").await;
    assert_eq!(
        reason(&file),
        WholeFileReason::MalformedMarkers { line: 11 }
    );
}

#[tokio::test]
async fn non_utf8_text_is_resolved_whole() {
    // Latin-1 "ñ" (0xF1) is not valid UTF-8.
    let repo = merge_conflict("f.txt", b"a\xf1o\n", b"main \xf1\n", b"feature \xf1\n").await;
    let file = read(&repo, "f.txt").await;
    assert_eq!(reason(&file), WholeFileReason::NotUtf8);
}

#[tokio::test]
async fn marker_size_comes_from_the_attributes() {
    let repo = TestRepo::new().await;
    repo.write_file(".gitattributes", "*.md conflict-marker-size=12\n");
    repo.commit_file("doc.md", "Title\n=======\nbase\n", "base")
        .await;
    repo.create_branch("feature").await;
    repo.checkout("feature").await;
    repo.commit_file("doc.md", "Title\n=======\nfeature\n", "feature")
        .await;
    repo.checkout("main").await;
    repo.commit_file("doc.md", "Title\n=======\nmain\n", "main")
        .await;
    stop_on_conflict(&repo, &["merge", "feature"]).await;

    let file = read(&repo, "doc.md").await;
    let ConflictContent::Blocks { marker_size, .. } = &file.content else {
        panic!("expected blocks");
    };
    assert_eq!(*marker_size, 12);
    let (blocks, _) = blocks(&file);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].ours, "main\n");
    assert_eq!(blocks[0].base.as_deref(), Some("base\n"));
}

#[tokio::test]
async fn unusual_paths() {
    for path in ["con espacio/ñandú.txt", "-dash.txt", "[glob].txt"] {
        let repo = merge_conflict(path, b"base\n", b"main\n", b"feature\n").await;
        let file = read(&repo, path).await;
        assert_eq!(file.path, path);
        let (blocks, _) = blocks(&file);
        assert_eq!(blocks[0].base.as_deref(), Some("base\n"), "{path}");
    }
}

#[tokio::test]
async fn files_not_in_conflict_are_refused() {
    let repo = merge_conflict("f.rs", BASE.as_bytes(), MAIN.as_bytes(), FEATURE.as_bytes()).await;
    repo.commit_file("other.txt", "x\n", "other").await;
    for path in ["other.txt", "missing.txt"] {
        let err = conflict_file(repo.git(), repo.path(), path, &c())
            .await
            .expect_err(path);
        assert_eq!(err.kind(), ErrorKind::NotConflicted, "{path}");
    }
    for path in ["../f.rs", "/f.rs"] {
        assert!(
            conflict_file(repo.git(), repo.path(), path, &c())
                .await
                .is_err()
        );
    }
}
