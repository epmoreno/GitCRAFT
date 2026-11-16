//! The history of one file (task 12.3), against real repositories and a
//! fixture of git's real output.

use std::path::PathBuf;
use std::time::Instant;

use gitcraft_core::ops::file_history::file_history;
use gitcraft_core::parse::diff_tree::FileStatus;
use gitcraft_core::parse::file_history::{
    FILE_HISTORY_FORMAT, FileHistoryEntry, parse_file_history,
};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, GitError};

fn c() -> CancellationToken {
    CancellationToken::new()
}

/// `notas.txt` is created, changed, renamed with a change to
/// `docs/guía de uso.txt`, changed again on a branch that is merged,
/// deleted and created again. Another file changes in between.
struct Sample {
    repo: TestRepo,
    created: String,
    changed: String,
    renamed: String,
    on_branch: String,
    deleted: String,
    again: String,
}

async fn sample() -> Sample {
    let repo = TestRepo::new().await;
    let created = repo
        .commit_file(
            "notas.txt",
            "uno\ndos\ntres\ncuatro\ncinco\n",
            "Crea las notas",
        )
        .await;
    repo.commit_file("otro.txt", "x\n", "Otro archivo").await;
    let changed = repo
        .commit_file(
            "notas.txt",
            "uno\ndos\ntres\ncuatro\ncinco\nseis\n",
            "Amplía las notas",
        )
        .await;
    std::fs::create_dir_all(repo.path().join("docs")).expect("mkdir");
    repo.run(&["mv", "notas.txt", "docs/guía de uso.txt"]).await;
    let renamed = repo
        .commit_file(
            "docs/guía de uso.txt",
            "uno\ndos\ntres\ncuatro\ncinco\nseis\nsiete\n",
            "Mueve las notas a docs",
        )
        .await;
    repo.run(&["switch", "--quiet", "-c", "rama"]).await;
    let on_branch = repo
        .commit_file(
            "docs/guía de uso.txt",
            "uno\ndos\ntres\ncuatro\ncinco\nseis\nsiete\nocho\n",
            "En la rama",
        )
        .await;
    repo.checkout("main").await;
    repo.commit_file("otro.txt", "y\n", "Cambia otro").await;
    repo.merge("rama", "Merge rama").await;
    repo.run(&["rm", "--quiet", "docs/guía de uso.txt"]).await;
    let deleted = repo.commit_all("Borra la guía").await;
    let again = repo
        .commit_file("docs/guía de uso.txt", "nueva\n", "La vuelve a crear")
        .await;
    Sample {
        repo,
        created,
        changed,
        renamed,
        on_branch,
        deleted,
        again,
    }
}

async fn history(repo: &TestRepo, path: &str, rev: &str) -> Vec<FileHistoryEntry> {
    let mut entries = Vec::new();
    let summary = file_history(repo.git(), repo.path(), path, rev, &c(), |batch| {
        entries.extend(batch)
    })
    .await
    .expect("file history");
    assert_eq!(summary.count as usize, entries.len());
    entries
}

#[tokio::test]
async fn follows_renames_deletions_and_merged_branches() {
    let s = sample().await;
    let entries = history(&s.repo, "docs/guía de uso.txt", "HEAD").await;
    let summary: Vec<_> = entries
        .iter()
        .map(|e| {
            (
                e.oid.as_str(),
                e.status,
                e.path.as_str(),
                e.old_path.as_deref(),
            )
        })
        .collect();
    let guide = "docs/guía de uso.txt";
    assert_eq!(
        summary,
        [
            (s.again.as_str(), FileStatus::Added, guide, None),
            (s.deleted.as_str(), FileStatus::Deleted, guide, None),
            (s.on_branch.as_str(), FileStatus::Modified, guide, None),
            (
                s.renamed.as_str(),
                FileStatus::Renamed,
                guide,
                Some("notas.txt")
            ),
            (s.changed.as_str(), FileStatus::Modified, "notas.txt", None),
            (s.created.as_str(), FileStatus::Added, "notas.txt", None),
        ]
    );
    assert_eq!(entries[3].summary, "Mueve las notas a docs");
    assert!(entries.iter().all(|e| e.author_time > 0));
}

#[tokio::test]
async fn from_another_commit_and_for_a_file_that_is_gone() {
    let s = sample().await;
    // From the commit that renamed it: only what came before.
    let entries = history(&s.repo, "docs/guía de uso.txt", &s.renamed).await;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].oid, s.renamed);
    // The old name, which no longer exists: its own history.
    let old = history(&s.repo, "notas.txt", "HEAD").await;
    let oids: Vec<_> = old.iter().map(|e| e.oid.as_str()).collect();
    assert_eq!(
        oids,
        [s.renamed.as_str(), s.changed.as_str(), s.created.as_str()]
    );
    assert_eq!(old[0].status, FileStatus::Deleted);
}

#[tokio::test]
async fn nothing_for_unknown_files_or_a_new_repository() {
    let s = sample().await;
    assert!(history(&s.repo, "no-existe.txt", "HEAD").await.is_empty());
    assert!(history(&s.repo, "", "HEAD").await.is_empty());
    let empty = TestRepo::new().await;
    assert!(history(&empty, "a.txt", "HEAD").await.is_empty());
}

#[tokio::test]
async fn refuses_a_start_that_is_not_a_commit_id() {
    let s = sample().await;
    for rev in ["--all", "main", "HEAD~1"] {
        let result =
            file_history(s.repo.git(), s.repo.path(), "notas.txt", rev, &c(), |_| {}).await;
        assert!(result.is_err(), "{rev}");
    }
}

#[tokio::test]
async fn paths_that_look_like_options_are_paths() {
    let repo = TestRepo::new().await;
    let commit = repo.commit_file("--all", "x\n", "Un nombre raro").await;
    let entries = history(&repo, "--all", "HEAD").await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].oid, commit);
}

// --- a long history ------------------------------------------------------------

const LONG: usize = 1_500;

async fn long_history() -> TestRepo {
    let repo = TestRepo::new().await;
    // fast-import: one file changed by every commit.
    let mut stream = String::new();
    for i in 1..=LONG {
        let content = format!("versión {i}\n");
        let message = format!("Cambio {i}");
        stream.push_str(&format!(
            "commit refs/heads/main\nmark :{i}\ncommitter T <t@gitcraft.invalid> {} +0000\ndata {}\n{message}\n",
            1_600_000_000 + i * 60,
            message.len()
        ));
        if i > 1 {
            stream.push_str(&format!("from :{}\n", i - 1));
        }
        stream.push_str(&format!(
            "M 100644 inline largo.txt\ndata {}\n{content}\n",
            content.len()
        ));
    }
    repo.write()
        .args(["fast-import", "--quiet"])
        .stdin(stream)
        .output(&c())
        .await
        .expect("fast-import");
    repo
}

#[tokio::test]
async fn arrives_in_batches_long_before_the_end() {
    let repo = long_history().await;
    let started = Instant::now();
    let mut batches = Vec::new();
    let summary = file_history(
        repo.git(),
        repo.path(),
        "largo.txt",
        "HEAD",
        &c(),
        |batch| batches.push((batch.len(), started.elapsed())),
    )
    .await
    .expect("history");
    let total = started.elapsed();
    assert_eq!(summary.count as usize, LONG);
    assert!(batches.len() > 1);
    assert!(batches[0].1 < total, "the first batch came before the end");
}

#[tokio::test]
async fn cancelling_stops_it() {
    let repo = long_history().await;
    let cancel = c();
    let mut received = 0;
    let result = file_history(
        repo.git(),
        repo.path(),
        "largo.txt",
        "HEAD",
        &cancel,
        |batch| {
            received += batch.len();
            cancel.cancel();
        },
    )
    .await;
    assert!(matches!(result, Err(GitError::Cancelled)), "{result:?}");
    assert!(received < LONG);
}

// --- git's real output -------------------------------------------------------

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/file-history.z")
}

/// Regenerate with `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test file_history`.
#[tokio::test]
async fn fixture_matches_current_git_output() {
    let s = sample().await;
    let fresh = s
        .repo
        .read()
        .args([
            "log",
            "--follow",
            "-z",
            "--name-status",
            FILE_HISTORY_FORMAT,
            "HEAD",
            "--",
            "docs/guía de uso.txt",
        ])
        .output(&c())
        .await
        .expect("log")
        .stdout_lossy();
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(fixture_path(), &fresh).expect("write fixture");
    }
    let fixture = std::fs::read_to_string(fixture_path()).expect("read fixture");
    assert!(fixture == fresh, "log output changed; see UPDATE_FIXTURES");
}

#[test]
fn parses_the_fixture() {
    let raw = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let entries = parse_file_history(&raw).expect("parse");
    let statuses: Vec<_> = entries.iter().map(|e| e.status).collect();
    assert_eq!(
        statuses,
        [
            FileStatus::Added,
            FileStatus::Deleted,
            FileStatus::Modified,
            FileStatus::Renamed,
            FileStatus::Modified,
            FileStatus::Added,
        ]
    );
    let renamed = &entries[3];
    assert_eq!(renamed.old_path.as_deref(), Some("notas.txt"));
    assert_eq!(renamed.path, "docs/guía de uso.txt");
    assert_eq!(renamed.author_name, "GitCRAFT Test");
}
