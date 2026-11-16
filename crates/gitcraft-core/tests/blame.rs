//! Blame (task 12.5), against real repositories and a fixture of git's real
//! output.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use gitcraft_core::ops::blame::{BlameOutcome, BlameStream, LARGE_LINES, blame};
use gitcraft_core::parse::blame::{BlameChunk, BlameCommit, BlameEvent, parse_blame, unquote};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, GitError};

fn c() -> CancellationToken {
    CancellationToken::new()
}

/// Everything a blame sent: the text, the commits and, per line (1-based),
/// the commit it comes from.
#[derive(Default)]
struct Blamed {
    lines: Vec<String>,
    commits: HashMap<String, BlameCommit>,
    chunks: Vec<BlameChunk>,
    batches: usize,
    attributed: u32,
}

impl Blamed {
    fn line_owner(&self, line: u32) -> &str {
        self.chunks
            .iter()
            .find(|c| line >= c.line && line < c.line + c.count)
            .map(|c| c.oid.as_str())
            .unwrap_or("")
    }
}

async fn run(repo: &TestRepo, path: &str, rev: &str, force: bool) -> (BlameOutcome, Blamed) {
    let mut blamed = Blamed::default();
    let outcome = blame(
        repo.git(),
        repo.path(),
        path,
        rev,
        force,
        &c(),
        |event| match event {
            BlameStream::Text { lines } => {
                assert!(blamed.chunks.is_empty(), "the text comes first");
                blamed.lines = lines;
            }
            BlameStream::Batch(batch) => {
                blamed.batches += 1;
                assert!(batch.attributed >= blamed.attributed);
                blamed.attributed = batch.attributed;
                for commit in batch.commits {
                    assert!(
                        blamed.commits.insert(commit.oid.clone(), commit).is_none(),
                        "each commit is sent once"
                    );
                }
                blamed.chunks.extend(batch.chunks);
            }
        },
    )
    .await
    .expect("blame");
    (outcome, blamed)
}

/// `guía.txt`: five lines by A; B changes the second; the file is renamed
/// to `docs/guía nueva.txt` with a new last line by C.
struct Sample {
    repo: TestRepo,
    a: String,
    b: String,
    c: String,
}

async fn sample() -> Sample {
    let repo = TestRepo::new().await;
    let a = repo
        .commit_file(
            "guía.txt",
            "uno\ndos\ntres\ncuatro\ncinco\n",
            "A: crea la guía",
        )
        .await;
    let b = repo
        .commit_file(
            "guía.txt",
            "uno\nDOS\ntres\ncuatro\ncinco\n",
            "B: cambia la dos",
        )
        .await;
    std::fs::create_dir_all(repo.path().join("docs")).expect("mkdir");
    repo.run(&["mv", "guía.txt", "docs/guía nueva.txt"]).await;
    let c = repo
        .commit_file(
            "docs/guía nueva.txt",
            "uno\nDOS\ntres\ncuatro\ncinco\nseis\n",
            "C: mueve y añade",
        )
        .await;
    Sample { repo, a, b, c }
}

#[tokio::test]
async fn attributes_every_line_through_a_rename() {
    let s = sample().await;
    let (outcome, blamed) = run(&s.repo, "docs/guía nueva.txt", "HEAD", false).await;
    assert_eq!(outcome, BlameOutcome::Done { lines: 6 });
    assert_eq!(
        blamed.lines,
        ["uno", "DOS", "tres", "cuatro", "cinco", "seis"]
    );
    assert_eq!(blamed.attributed, 6);
    let owners: Vec<_> = (1..=6).map(|l| blamed.line_owner(l)).collect();
    assert_eq!(
        owners,
        [&s.a, &s.b, &s.a, &s.a, &s.a, &s.c].map(String::as_str)
    );
    // A is in two groups (around B's line) but sent once.
    assert_eq!(blamed.commits.len(), 3);
    let a = &blamed.commits[&s.a];
    assert_eq!(a.summary, "A: crea la guía");
    assert!(a.boundary, "nothing before the root commit");
    // A's lines were in the old file: their path then, and "previous" for C.
    let a_chunk = blamed.chunks.iter().find(|c| c.oid == s.a).expect("A");
    assert_eq!(a_chunk.path, "guía.txt");
    assert!(a_chunk.previous.is_none());
    let b_chunk = blamed.chunks.iter().find(|c| c.oid == s.b).expect("B");
    let previous = b_chunk.previous.as_ref().expect("previous");
    assert_eq!(
        (previous.oid.as_str(), previous.path.as_str()),
        (s.a.as_str(), "guía.txt")
    );
}

#[tokio::test]
async fn at_an_older_commit_and_before_a_commit() {
    let s = sample().await;
    let (_, blamed) = run(&s.repo, "guía.txt", &s.b, false).await;
    assert_eq!(blamed.lines.len(), 5);
    assert_eq!(blamed.line_owner(2), s.b);
    // "Blame before this commit": its previous commit and path.
    let (_, before) = run(&s.repo, "guía.txt", &s.a, false).await;
    assert!((1..=5).all(|l| before.line_owner(l) == s.a));
    let _ = &s.c;
}

#[tokio::test]
async fn respects_the_repository_ignore_revs_file() {
    let s = sample().await;
    // A commit that only reformats a line, then listed as ignored.
    let reformat = s
        .repo
        .commit_file(
            "docs/guía nueva.txt",
            "uno \nDOS\ntres\ncuatro\ncinco\nseis\n",
            "Formato",
        )
        .await;
    let (_, blamed) = run(&s.repo, "docs/guía nueva.txt", "HEAD", false).await;
    assert_eq!(blamed.line_owner(1), reformat);
    s.repo
        .write_file(".git-blame-ignore-revs", &format!("{reformat}\n"));
    s.repo
        .run(&["config", "blame.ignoreRevsFile", ".git-blame-ignore-revs"])
        .await;
    let (_, blamed) = run(&s.repo, "docs/guía nueva.txt", "HEAD", false).await;
    assert_eq!(blamed.line_owner(1), s.a);
}

#[tokio::test]
async fn refuses_binary_missing_and_huge_files_and_asks_for_big_ones() {
    let repo = TestRepo::new().await;
    std::fs::write(repo.path().join("imagen.bin"), b"PNG\0\x01\x02").expect("write");
    let big: String = (0..=LARGE_LINES).map(|i| format!("línea {i}\n")).collect();
    repo.commit_file("grande.txt", &big, "Grande").await;
    let (outcome, blamed) = run(&repo, "imagen.bin", "HEAD", false).await;
    assert_eq!(outcome, BlameOutcome::Binary);
    assert!(blamed.lines.is_empty());
    let (outcome, _) = run(&repo, "no-existe.txt", "HEAD", false).await;
    assert_eq!(outcome, BlameOutcome::Missing);
    let (outcome, _) = run(&repo, "", "HEAD", false).await;
    assert_eq!(
        outcome,
        BlameOutcome::Missing,
        "the root folder is not a file"
    );

    let (outcome, blamed) = run(&repo, "grande.txt", "HEAD", false).await;
    assert_eq!(
        outcome,
        BlameOutcome::Large {
            lines: LARGE_LINES + 1
        }
    );
    assert!(blamed.lines.is_empty(), "nothing sent before agreeing");
    let (outcome, blamed) = run(&repo, "grande.txt", "HEAD", true).await;
    assert_eq!(
        outcome,
        BlameOutcome::Done {
            lines: LARGE_LINES + 1
        }
    );
    assert_eq!(blamed.attributed, LARGE_LINES + 1);
}

#[tokio::test]
async fn refuses_a_start_that_is_not_a_commit_id() {
    let s = sample().await;
    for rev in ["--all", "main", "HEAD~1"] {
        let result = blame(
            s.repo.git(),
            s.repo.path(),
            "guía.txt",
            rev,
            false,
            &c(),
            |_| {},
        )
        .await;
        assert!(result.is_err(), "{rev}");
    }
}

#[tokio::test]
async fn an_empty_file_has_nothing_to_blame() {
    let repo = TestRepo::new().await;
    repo.commit_file("vacío.txt", "", "Vacío").await;
    let (outcome, blamed) = run(&repo, "vacío.txt", "HEAD", false).await;
    assert_eq!(outcome, BlameOutcome::Done { lines: 0 });
    assert!(blamed.lines.is_empty() && blamed.chunks.is_empty());
}

// --- a long history ------------------------------------------------------------

const COMMITS: usize = 1_500;

/// One file, each commit appending a line: as many groups as commits.
async fn grown_file() -> TestRepo {
    let repo = TestRepo::new().await;
    let mut stream = String::new();
    let mut content = String::new();
    for i in 1..=COMMITS {
        content.push_str(&format!("línea {i}\n"));
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
    let repo = grown_file().await;
    let started = Instant::now();
    let mut first_batch = None;
    let mut attributed = Vec::new();
    let outcome = blame(
        repo.git(),
        repo.path(),
        "largo.txt",
        "HEAD",
        false,
        &c(),
        |event| {
            if let BlameStream::Batch(batch) = event {
                first_batch.get_or_insert(started.elapsed());
                attributed.push(batch.attributed);
            }
        },
    )
    .await
    .expect("blame");
    let total = started.elapsed();
    assert_eq!(
        outcome,
        BlameOutcome::Done {
            lines: COMMITS as u32
        }
    );
    assert!(attributed.len() > 1, "{attributed:?}");
    assert!((attributed[0] as usize) < COMMITS);
    assert_eq!(*attributed.last().expect("last") as usize, COMMITS);
    assert!(first_batch.expect("a batch") < total);
}

#[tokio::test]
async fn cancelling_stops_it() {
    let repo = grown_file().await;
    let cancel = c();
    let mut attributed = 0;
    let result = blame(
        repo.git(),
        repo.path(),
        "largo.txt",
        "HEAD",
        false,
        &cancel,
        |event| {
            if let BlameStream::Batch(batch) = event {
                attributed = batch.attributed;
                cancel.cancel();
            }
        },
    )
    .await;
    assert!(matches!(result, Err(GitError::Cancelled)), "{result:?}");
    assert!((attributed as usize) < COMMITS);
}

// --- git's real output -------------------------------------------------------

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blame-incremental.txt")
}

/// Regenerate with `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test blame`.
#[tokio::test]
async fn fixture_matches_current_git_output() {
    let s = sample().await;
    let fresh = s
        .repo
        .read()
        .args([
            "blame",
            "--incremental",
            "HEAD",
            "--",
            "docs/guía nueva.txt",
        ])
        .output(&c())
        .await
        .expect("blame")
        .stdout_lossy();
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(fixture_path(), &fresh).expect("write fixture");
    }
    let fixture = std::fs::read_to_string(fixture_path()).expect("read fixture");
    assert!(
        fixture == fresh,
        "blame output changed; see UPDATE_FIXTURES"
    );
}

#[test]
fn parses_the_fixture() {
    let raw = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let events = parse_blame(&raw).expect("parse");
    let commits = events
        .iter()
        .filter(|e| matches!(e, BlameEvent::Commit(_)))
        .count();
    let chunks: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            BlameEvent::Chunk(chunk) => Some(chunk),
            BlameEvent::Commit(_) => None,
        })
        .collect();
    assert_eq!(commits, 3);
    assert_eq!(chunks.len(), 4, "A's lines in two groups");
    assert_eq!(chunks.iter().map(|c| c.count).sum::<u32>(), 6);
    assert!(chunks.iter().any(|c| c.path == "guía.txt"));
    assert!(chunks.iter().any(|c| c.path == "docs/guía nueva.txt"));
}

#[test]
fn unquotes_paths_git_quoted() {
    assert_eq!(unquote("plain name.txt"), "plain name.txt");
    assert_eq!(unquote(r#""tab\there.txt""#), "tab\there.txt");
    assert_eq!(unquote(r#""quote\"d\\.txt""#), "quote\"d\\.txt");
    // Octal bytes of UTF-8 (what git writes without core.quotepath=false).
    assert_eq!(unquote(r#""gu\303\255a.txt""#), "guía.txt");
}
