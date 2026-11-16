//! Search, file history and blame benchmark (task 12.7; ignored by
//! default). Run it in release mode:
//!
//! ```sh
//! npm run bench:search
//! # with a real repository too (FILE: a file with a long history there):
//! GITCRAFT_BENCH_REPO=~/src/git GITCRAFT_BENCH_FILE=diff.c npm run bench:search
//! ```
//!
//! Targets (docs/ROADMAP.md, task 12): message search, first result
//! < 300 ms and all < 3 s; file history, first page < 500 ms with a
//! commit-graph; blame, first lines < 1 s. Times are from spawning git to
//! the first batch and to the end; the median of three runs.

use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use gitcraft_core::ops::blame::{BlameOutcome, BlameStream, blame};
use gitcraft_core::ops::file_history::file_history;
use gitcraft_core::ops::search::{SearchField, SearchQuery, search};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, Git};

const RUNS: usize = 3;
const FILES: usize = 200;
/// The file every tenth commit changes (file history and blame).
const BIG_FILE: &str = "src/grande.txt";

#[derive(Clone, Copy)]
struct Timing {
    first: Option<Duration>,
    total: Duration,
    found: usize,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

async fn median<F, Fut>(mut run: F) -> Timing
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Timing>,
{
    let mut runs = Vec::new();
    for _ in 0..RUNS {
        runs.push(run().await);
    }
    runs.sort_by_key(|r| r.total);
    runs[RUNS / 2]
}

fn report(label: &str, t: Timing) {
    let first = t
        .first
        .map_or("—".to_owned(), |f| format!("{:.0} ms", ms(f)));
    println!(
        "  {label:<44} first {first:>8} | all {:>7.0} ms | {} found",
        ms(t.total),
        t.found
    );
}

async fn time_search(git: &Git, repo: &Path, query: SearchQuery) -> Timing {
    median(|| {
        let query = query.clone();
        async move {
            let started = Instant::now();
            let mut first = None;
            let summary = search(git, repo, &query, false, &CancellationToken::new(), |b| {
                if !b.oids.is_empty() {
                    first.get_or_insert_with(|| started.elapsed());
                }
            })
            .await
            .expect("search");
            Timing {
                first,
                total: started.elapsed(),
                found: summary.found as usize,
            }
        }
    })
    .await
}

async fn time_history(git: &Git, repo: &Path, path: &str) -> Timing {
    median(|| async move {
        let started = Instant::now();
        let mut first = None;
        let summary = file_history(git, repo, path, "HEAD", &CancellationToken::new(), |_| {
            first.get_or_insert_with(|| started.elapsed());
        })
        .await
        .expect("file history");
        Timing {
            first,
            total: started.elapsed(),
            found: summary.count as usize,
        }
    })
    .await
}

async fn time_blame(git: &Git, repo: &Path, path: &str) -> Timing {
    median(|| async move {
        let started = Instant::now();
        let mut first = None;
        let outcome = blame(
            git,
            repo,
            path,
            "HEAD",
            true,
            &CancellationToken::new(),
            |e| {
                if matches!(e, BlameStream::Batch(_)) {
                    first.get_or_insert_with(|| started.elapsed());
                }
            },
        )
        .await
        .expect("blame");
        let BlameOutcome::Done { lines } = outcome else {
            panic!("blame of {path}: {outcome:?}");
        };
        Timing {
            first,
            total: started.elapsed(),
            found: lines as usize,
        }
    })
    .await
}

fn query(field: SearchField, text: &str, regex: bool) -> SearchQuery {
    SearchQuery {
        text: text.to_owned(),
        field,
        regex,
        match_case: false,
    }
}

async fn measure(git: &Git, repo: &Path, file: &str, rare: &str, common: &str) {
    report(
        &format!("message «{common}» (literal)"),
        time_search(git, repo, query(SearchField::Message, common, false)).await,
    );
    report(
        &format!("message «{rare}» (literal, whole history)"),
        time_search(git, repo, query(SearchField::Message, rare, false)).await,
    );
    report(
        &format!("message «^{common}» (regex)"),
        time_search(
            git,
            repo,
            query(SearchField::Message, &format!("^{common}"), true),
        )
        .await,
    );
    report(
        "author «a» (literal)",
        time_search(git, repo, query(SearchField::Author, "a", false)).await,
    );
    report(
        &format!("file «{file}»"),
        time_search(git, repo, query(SearchField::Path, file, false)).await,
    );
    report(
        &format!("history of {file}"),
        time_history(git, repo, file).await,
    );
    report(
        &format!("blame of {file} (lines)"),
        time_blame(git, repo, file).await,
    );
}

/// `commits` commits, each changing one of [`FILES`] files and every
/// tenth one also [`BIG_FILE`] (a line appended), with varied authors.
async fn synthetic(commits: usize) -> TestRepo {
    let repo = TestRepo::new().await;
    let mut stream = String::with_capacity(commits * 200);
    let mut big = String::new();
    for i in 1..=commits {
        let author = ["Ana", "Luis", "Marta", "Pere"][i % 4];
        let message = format!("change {i}: tweak file {}", i % FILES);
        let _ = write!(
            stream,
            "commit refs/heads/main\nmark :{i}\nauthor {author} <{}@gitcraft.invalid> {} +0000\ncommitter {author} <{}@gitcraft.invalid> {} +0000\ndata {}\n{message}\n",
            author.to_lowercase(),
            1_600_000_000 + i * 60,
            author.to_lowercase(),
            1_600_000_000 + i * 60,
            message.len()
        );
        if i > 1 {
            let _ = writeln!(stream, "from :{}", i - 1);
        }
        let small = format!("version {i}\n");
        let _ = write!(
            stream,
            "M 100644 inline src/file-{}.txt\ndata {}\n{small}\n",
            i % FILES,
            small.len()
        );
        if i % 10 == 0 {
            let _ = writeln!(big, "line {i} by {author}");
            let _ = write!(
                stream,
                "M 100644 inline {BIG_FILE}\ndata {}\n{big}\n",
                big.len()
            );
        }
    }
    repo.write()
        .args(["fast-import", "--quiet"])
        .stdin(stream)
        .output(&CancellationToken::new())
        .await
        .expect("fast-import");
    repo
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "benchmark: run with `npm run bench:search`"]
async fn bench_search_history_and_blame() {
    let git = gitcraft_core::testing::isolated_git();
    let commits: usize = std::env::var("GITCRAFT_BENCH_COMMITS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_000);

    let started = Instant::now();
    let repo = synthetic(commits).await;
    println!(
        "git {} — generated {commits} synthetic commits in {:.1} s",
        git.version().await.expect("version"),
        started.elapsed().as_secs_f64()
    );
    println!("synthetic, no commit-graph:");
    measure(
        &git,
        repo.path(),
        BIG_FILE,
        "not there anywhere",
        "change 4",
    )
    .await;
    repo.run(&["commit-graph", "write", "--reachable", "--changed-paths"])
        .await;
    println!("synthetic, with commit-graph --changed-paths:");
    measure(
        &git,
        repo.path(),
        BIG_FILE,
        "not there anywhere",
        "change 4",
    )
    .await;

    if let Some(real) = std::env::var_os("GITCRAFT_BENCH_REPO") {
        let real = Path::new(&real);
        let file = std::env::var("GITCRAFT_BENCH_FILE").unwrap_or_else(|_| "README.md".into());
        println!("{}:", real.display());
        measure(&git, real, &file, "not there anywhere", "fix").await;
    }
}
