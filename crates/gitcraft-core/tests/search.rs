//! Searching the history (task 12.1), against real repositories, a fixture
//! of git's real output and a large synthetic history.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gitcraft_core::ops::search::{
    BATCH_RESULTS, SearchBatch, SearchField, SearchQuery, SearchSummary, exact_path, path_pathspec,
    search,
};
use gitcraft_core::parse::search::{MESSAGE_FORMAT, PATH_FORMAT, PathRecords, parse_search_record};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind, GitError};

fn c() -> CancellationToken {
    CancellationToken::new()
}

fn query(field: SearchField, text: &str) -> SearchQuery {
    SearchQuery {
        text: text.to_owned(),
        field,
        regex: false,
        match_case: false,
    }
}

/// Runs a search to the end: every result, in order, and the summary.
async fn run(repo: &TestRepo, query: &SearchQuery) -> (Vec<String>, SearchSummary) {
    let mut oids = Vec::new();
    let summary = search(repo.git(), repo.path(), query, false, &c(), |batch| {
        oids.extend(batch.oids)
    })
    .await
    .expect("search");
    assert_eq!(summary.found as usize, oids.len());
    (oids, summary)
}

async fn found(repo: &TestRepo, query: &SearchQuery) -> Vec<String> {
    run(repo, query).await.0
}

/// main: four commits with varied messages and two authors; `side`, merged,
/// changes a file in a folder; `lost` is a commit no branch has.
struct Sample {
    repo: TestRepo,
    first: String,
    fix: String,
    accents: String,
    dashes: String,
    side: String,
    lost: String,
}

async fn sample() -> Sample {
    let repo = TestRepo::new().await;
    let first = repo
        .commit_file(
            "README.md",
            "hola\n",
            "Primer commit\n\nCon un cuerpo que menciona la Palabra clave.",
        )
        .await;
    repo.write_file("src/ui/Tooltip.tsx", "export {}\n");
    repo.run(&["add", "--all"]).await;
    let fix = repo
        .commit_with(&[
            "--author=Ana Pérez <ana@example.invalid>",
            "-m",
            "fix: el tooltip se corta",
        ])
        .await;
    let accents = repo
        .commit_file("docs/Guía [beta].md", "guía\n", "Añade la GUÍA")
        .await;
    let dashes = repo
        .commit_file("notes.txt", "n\n", "--all no es una opción aquí")
        .await;
    repo.run(&["switch", "--quiet", "-c", "side"]).await;
    let side = repo
        .commit_file("src/ui/Tooltip.tsx", "export const a = 1;\n", "Lado")
        .await;
    repo.checkout("main").await;
    repo.merge("side", "Merge side").await;
    repo.run(&["switch", "--quiet", "-c", "tmp"]).await;
    let lost = repo.commit_file("lost.txt", "x\n", "Perdido").await;
    repo.checkout("main").await;
    repo.run(&["branch", "-D", "tmp"]).await;
    Sample {
        repo,
        first,
        fix,
        accents,
        dashes,
        side,
        lost,
    }
}

#[tokio::test]
async fn message_literal_ignores_case_and_looks_in_the_body() {
    let s = sample().await;
    let q = query(SearchField::Message, "palabra CLAVE");
    assert_eq!(found(&s.repo, &q).await, std::slice::from_ref(&s.first));
    let exact = SearchQuery {
        match_case: true,
        ..q
    };
    assert!(found(&s.repo, &exact).await.is_empty());
    let exact = SearchQuery {
        text: "Palabra clave".into(),
        ..exact
    };
    assert_eq!(found(&s.repo, &exact).await, std::slice::from_ref(&s.first));
}

#[tokio::test]
async fn message_literal_handles_accents_dashes_and_special_characters() {
    let s = sample().await;
    // Unicode case folding: "guía" finds "GUÍA".
    assert_eq!(
        found(&s.repo, &query(SearchField::Message, "guía")).await,
        std::slice::from_ref(&s.accents)
    );
    // Text starting with dashes is text, not an option.
    assert_eq!(
        found(&s.repo, &query(SearchField::Message, "--all")).await,
        std::slice::from_ref(&s.dashes)
    );
    // Regex characters are literal unless asked.
    assert!(
        found(&s.repo, &query(SearchField::Message, "fix.*corta"))
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn literal_searches_say_how_far_they_got() {
    let s = sample().await;
    let (_, summary) = run(&s.repo, &query(SearchField::Message, "fix")).await;
    // Every commit of the graph (not the lost one): 6.
    assert_eq!(summary.examined, 6);
    assert_eq!(summary.found, 1);
    let (_, by_file) = run(&s.repo, &query(SearchField::Path, "ui")).await;
    assert_eq!(by_file.examined, 0, "git filters: no progress");
}

#[tokio::test]
async fn message_regex_is_passed_to_git() {
    let s = sample().await;
    let q = SearchQuery {
        regex: true,
        ..query(SearchField::Message, "^FIX: .*corta$")
    };
    assert_eq!(found(&s.repo, &q).await, std::slice::from_ref(&s.fix));
    let exact = SearchQuery {
        match_case: true,
        ..q.clone()
    };
    assert!(found(&s.repo, &exact).await.is_empty());

    let bad = SearchQuery {
        text: "a(".into(),
        ..q
    };
    let err = search(s.repo.git(), s.repo.path(), &bad, false, &c(), |_| {})
        .await
        .expect_err("bad regex");
    assert_eq!(err.kind(), ErrorKind::InvalidPattern);
}

#[tokio::test]
async fn author_by_name_or_email_literal_or_regex() {
    let s = sample().await;
    assert_eq!(
        found(&s.repo, &query(SearchField::Author, "pérez")).await,
        std::slice::from_ref(&s.fix)
    );
    assert_eq!(
        found(&s.repo, &query(SearchField::Author, "ana@example")).await,
        std::slice::from_ref(&s.fix)
    );
    let regex = SearchQuery {
        regex: true,
        ..query(SearchField::Author, "^ana p")
    };
    assert_eq!(found(&s.repo, &regex).await, std::slice::from_ref(&s.fix));
    let bad = SearchQuery {
        text: "[".into(),
        ..regex
    };
    let err = search(s.repo.git(), s.repo.path(), &bad, false, &c(), |_| {})
        .await
        .expect_err("bad regex");
    assert_eq!(err.kind(), ErrorKind::InvalidPattern);
}

#[tokio::test]
async fn hash_by_prefix_and_outside_every_branch() {
    let s = sample().await;
    let prefix = s.side[..7].to_uppercase();
    let (oids, summary) = run(&s.repo, &query(SearchField::Hash, &prefix)).await;
    assert_eq!(oids, std::slice::from_ref(&s.side));
    assert_eq!(summary.outside, None);

    // Too short, or not an id: nothing is run.
    for text in ["abc", "zzzzzzz", "-all"] {
        let (oids, summary) = run(&s.repo, &query(SearchField::Hash, text)).await;
        assert!(oids.is_empty() && summary.examined == 0, "{text}");
    }

    // Only in the branch history: said, not listed.
    let (oids, summary) = run(&s.repo, &query(SearchField::Hash, &s.lost[..10])).await;
    assert!(oids.is_empty());
    assert_eq!(summary.outside.as_deref(), Some(s.lost.as_str()));
}

#[tokio::test]
async fn by_file_finds_every_commit_that_changed_it() {
    let s = sample().await;
    // Across folders, ignoring case, including the side branch.
    let mut oids = found(&s.repo, &query(SearchField::Path, "SRC/ui/tool")).await;
    oids.sort();
    let mut expected = vec![s.fix.clone(), s.side.clone()];
    expected.sort();
    assert!(
        expected.iter().all(|oid| oids.contains(oid)),
        "{oids:?} should have {expected:?}"
    );
    assert!(!oids.contains(&s.first) && !oids.contains(&s.accents));

    // Brackets and accents are literal.
    assert_eq!(
        found(&s.repo, &query(SearchField::Path, "guía [beta]")).await,
        std::slice::from_ref(&s.accents)
    );
    let exact = SearchQuery {
        match_case: true,
        ..query(SearchField::Path, "src/UI")
    };
    assert!(found(&s.repo, &exact).await.is_empty());
}

#[tokio::test]
async fn by_file_an_exact_path_is_only_that_one() {
    let repo = TestRepo::new().await;
    let root = repo.commit_file("diff.c", "a\n", "Root diff").await;
    let builtin = repo
        .commit_file("builtin/diff.c", "b\n", "Builtin diff")
        .await;
    let guide = repo.commit_file("Docs/Guía [x].md", "g\n", "Guide").await;
    let dashes = repo.commit_file("-rf.txt", "d\n", "Dashes").await;
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };

    // A tracked file: only it, not every path containing the text.
    assert_eq!(
        found(&repo, &query(SearchField::Path, "diff.c")).await,
        std::slice::from_ref(&root)
    );
    // Spelled as in the index without "match case"; with it, no such path
    // (and none contains it).
    assert_eq!(
        found(&repo, &query(SearchField::Path, "DIFF.C")).await,
        std::slice::from_ref(&root)
    );
    let cased = SearchQuery {
        match_case: true,
        ..query(SearchField::Path, "DIFF.C")
    };
    assert!(found(&repo, &cased).await.is_empty());
    // A folder, with or without the slash.
    for text in ["builtin", "builtin/", "BUILTIN"] {
        assert_eq!(
            found(&repo, &query(SearchField::Path, text)).await,
            std::slice::from_ref(&builtin),
            "{text}"
        );
    }
    // Not a path: every path containing it.
    assert_eq!(
        sorted(found(&repo, &query(SearchField::Path, "iff.c")).await),
        sorted(vec![root.clone(), builtin.clone()])
    );
    // Brackets, accents and a leading dash are literal.
    assert_eq!(
        found(&repo, &query(SearchField::Path, "docs/guía [x].md")).await,
        std::slice::from_ref(&guide)
    );
    assert_eq!(
        exact_path(repo.git(), repo.path(), "docs/guía [x].md", false, &c())
            .await
            .expect("exact path")
            .as_deref(),
        Some("Docs/Guía [x].md")
    );
    assert_eq!(
        found(&repo, &query(SearchField::Path, "-rf.txt")).await,
        std::slice::from_ref(&dashes)
    );
    // "diff" is a prefix of "diff.c", not a path.
    assert_eq!(
        exact_path(repo.git(), repo.path(), "diff", false, &c())
            .await
            .expect("exact path"),
        None
    );

    // Deleted: no longer tracked, so its whole history by "contains".
    repo.run(&["rm", "--quiet", "diff.c"]).await;
    let removed = repo.commit_with(&["-m", "Remove diff.c"]).await;
    assert_eq!(
        sorted(found(&repo, &query(SearchField::Path, "diff.c")).await),
        sorted(vec![root, builtin, removed])
    );
}

/// A file changed on a branch merged twice: the first merge only brings
/// the branch's change (the same file as on the branch); the second one
/// joins changes from both sides (a file neither side had).
struct Merges {
    repo: TestRepo,
    /// In the order the search lists them (newest first).
    changed: Vec<String>,
    brings_one_side: String,
}

async fn merges_sample() -> Merges {
    let repo = TestRepo::new().await;
    let base = repo.commit_file("f.txt", "1\n2\n3\n4\n5\n", "Base").await;
    repo.run(&["switch", "--quiet", "-c", "side"]).await;
    let side_one = repo
        .commit_file("f.txt", "UNO\n2\n3\n4\n5\n", "Side: one")
        .await;
    repo.checkout("main").await;
    repo.commit_file("other.txt", "o\n", "Main: other file")
        .await;
    let brings_one_side = repo.merge("side", "Merge: brings side").await;
    repo.run(&["switch", "--quiet", "side"]).await;
    let side_five = repo
        .commit_file("f.txt", "UNO\n2\n3\n4\nCINCO\n", "Side: five")
        .await;
    repo.checkout("main").await;
    let main_three = repo
        .commit_file("f.txt", "UNO\n2\nTRES\n4\n5\n", "Main: three")
        .await;
    let joins_both = repo.merge("side", "Merge: joins both").await;
    Merges {
        repo,
        changed: vec![joins_both, main_three, side_five, side_one, base],
        brings_one_side,
    }
}

#[tokio::test]
async fn by_file_leaves_out_merges_that_only_bring_one_side() {
    let m = merges_sample().await;
    for text in ["f.txt", "f.t"] {
        let mut oids = found(&m.repo, &query(SearchField::Path, text)).await;
        assert!(!oids.contains(&m.brings_one_side), "{text}");
        oids.sort();
        let mut expected = m.changed.clone();
        expected.sort();
        assert_eq!(oids, expected, "{text}");
    }
}

#[test]
fn pathspec_escapes_wildcards() {
    assert_eq!(
        path_pathspec("a*b?[c]\\", false),
        ":(top,icase)*a\\*b\\?\\[c\\]\\\\*"
    );
    assert_eq!(path_pathspec("src/ui", true), ":(top)*src/ui*");
}

#[tokio::test]
async fn an_empty_repository_or_text_finds_nothing() {
    let repo = TestRepo::new().await;
    for field in [SearchField::Message, SearchField::Path] {
        let (oids, _) = run(&repo, &query(field, "x")).await;
        assert!(oids.is_empty());
    }
    let s = sample().await;
    let (oids, summary) = run(&s.repo, &query(SearchField::Message, "   ")).await;
    assert!(oids.is_empty() && summary.examined == 0);
}

// --- git's real output -------------------------------------------------------

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/search-message.z")
}

/// Regenerate with `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test search`.
#[tokio::test]
async fn fixture_matches_current_git_output() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "Uno").await;
    repo.commit_file(
        "b.txt",
        "b\n",
        "Dos: con cuerpo\n\nLínea uno del cuerpo.\n\nY otra con \u{1f} raro.",
    )
    .await;
    let fresh = repo
        .read()
        .args(["log", "-z", MESSAGE_FORMAT, "--branches", "--"])
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
    let records: Vec<_> = raw
        .split('\0')
        .filter(|r| !r.is_empty())
        .map(|r| parse_search_record(r).expect("parse"))
        .collect();
    assert_eq!(records.len(), 2);
    assert!(records[0].text.starts_with("Dos: con cuerpo\n\nLínea uno"));
    // A U+001F inside the message stays in the text.
    assert!(records[0].text.contains("con \u{1f} raro"));
    assert!(records[1].text.starts_with("Uno"));
    assert!(records.iter().all(|r| r.oid.len() == 40));
}

fn path_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/search-path.z")
}

/// Regenerate with `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test search`.
#[tokio::test]
async fn path_fixture_matches_current_git_output() {
    let m = merges_sample().await;
    let fresh = m
        .repo
        .read()
        .args([
            "log",
            "-z",
            PATH_FORMAT,
            "--full-history",
            "-c",
            "--name-only",
        ])
        .args(["--branches", "--", "f.txt"])
        .output(&c())
        .await
        .expect("log")
        .stdout_lossy();
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(path_fixture(), &fresh).expect("write fixture");
    }
    let fixture = std::fs::read_to_string(path_fixture()).expect("read fixture");
    assert!(fixture == fresh, "log output changed; see UPDATE_FIXTURES");
    let mut records = PathRecords::default();
    let mut oids: Vec<String> = fresh
        .split('\0')
        .filter_map(|t| records.push(t).expect("parse"))
        .collect();
    oids.extend(records.finish());
    assert_eq!(oids, m.changed);
}

#[test]
fn parses_the_path_fixture() {
    let raw = std::fs::read_to_string(path_fixture()).expect("read fixture");
    let mut records = PathRecords::default();
    let mut oids: Vec<String> = raw
        .split('\0')
        .filter_map(|t| records.push(t).expect("parse"))
        .collect();
    oids.extend(records.finish());
    // Six commits touch it; the merge that only brings one side is left out.
    assert_eq!(raw.matches('\u{1e}').count(), 6);
    assert_eq!(oids.len(), 5);
    assert!(oids.iter().all(|oid| oid.len() == 40));
    assert!(records.push("\u{1e}not an id").is_err());
}

// --- a large history -----------------------------------------------------------

const LARGE: usize = 50_000;

#[tokio::test]
async fn results_arrive_in_batches_long_before_the_end() {
    let repo = TestRepo::synthetic(LARGE).await;
    let started = Instant::now();
    let mut batches: Vec<(SearchBatch, Duration)> = Vec::new();
    let summary = search(
        repo.git(),
        repo.path(),
        &query(SearchField::Message, "change"),
        false,
        &c(),
        |batch| batches.push((batch, started.elapsed())),
    )
    .await
    .expect("search");
    let total = started.elapsed();
    assert_eq!(summary.found as usize, LARGE);
    assert_eq!(summary.examined as usize, LARGE);
    let (first, at) = &batches[0];
    assert!(first.oids.len() <= BATCH_RESULTS);
    assert!((first.examined as usize) < LARGE);
    assert!(*at < total, "the first batch came before the end");
    assert!(batches.len() >= LARGE / BATCH_RESULTS);
    // Found and examined only grow.
    assert!(
        batches
            .windows(2)
            .all(|w| w[0].0.found <= w[1].0.found && w[0].0.examined <= w[1].0.examined)
    );
}

#[tokio::test]
async fn cancelling_stops_it_and_leaves_no_git_running() {
    let repo = TestRepo::synthetic(LARGE).await;
    let cancel = c();
    let mut received = 0usize;
    let started = Instant::now();
    let result = search(
        repo.git(),
        repo.path(),
        &query(SearchField::Message, "change"),
        false,
        &cancel,
        |batch| {
            received += batch.oids.len();
            cancel.cancel();
        },
    )
    .await;
    assert!(matches!(result, Err(GitError::Cancelled)), "{result:?}");
    assert!(received < LARGE, "stopped before the end");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_no_git_in(repo.path());
}

/// No git process is left working in `dir` (Unix: by its working
/// directory, with `lsof`; skipped where `lsof` is missing).
fn assert_no_git_in(dir: &Path) {
    #[cfg(unix)]
    {
        let dir = std::fs::canonicalize(dir).expect("canonical");
        for _ in 0..40 {
            let Ok(out) = std::process::Command::new("lsof")
                .args(["-a", "-c", "git", "-d", "cwd", "-Fn"])
                .output()
            else {
                eprintln!("lsof not available; skipping the process check");
                return;
            };
            let listed = String::from_utf8_lossy(&out.stdout);
            let here = listed
                .lines()
                .filter_map(|l| l.strip_prefix('n'))
                .any(|path| Path::new(path) == dir);
            if !here {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("a git process is still running in {}", dir.display());
    }
    #[cfg(not(unix))]
    let _ = dir;
}
