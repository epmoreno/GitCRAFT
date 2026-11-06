//! Commit graph benchmark (ignored by default). Run it in release mode:
//!
//! ```sh
//! npm run bench:graph
//! # or with options:
//! GITCRAFT_BENCH_COMMITS=200000 GITCRAFT_BENCH_REPO=~/src/linux npm run bench:graph
//! ```
//!
//! Targets (docs/ROADMAP.md, task 2.4): first page < 150 ms and the whole
//! 100k-commit history < 3 s, from spawning git to the last laid-out row.
//! JSON encoding (what crosses the IPC bridge) is reported separately.

use std::path::Path;
use std::time::{Duration, Instant};

use gitcraft_core::graph::{GraphLine, load_graph};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, Git};

const RUNS: usize = 3;

struct Run {
    first_page: Duration,
    total: Duration,
    commits: usize,
    max_column: u16,
    json_bytes: usize,
    json: Duration,
}

async fn run_once(git: &Git, repo: &Path) -> Run {
    let started = Instant::now();
    let mut first_page = None;
    let mut pages: Vec<Vec<GraphLine>> = Vec::new();
    let commits = load_graph(git, repo, false, &CancellationToken::new(), |page| {
        first_page.get_or_insert_with(|| started.elapsed());
        pages.push(page);
    })
    .await
    .expect("load graph");
    let total = started.elapsed();

    let json_started = Instant::now();
    let json_bytes = pages
        .iter()
        .map(|p| serde_json::to_vec(p).expect("json").len())
        .sum();
    let json = json_started.elapsed();

    let max_column = pages.iter().flatten().map(|e| e.column).max().unwrap_or(0);
    Run {
        first_page: first_page.unwrap_or(total),
        total,
        commits,
        max_column,
        json_bytes,
        json,
    }
}

async fn measure(git: &Git, repo: &Path, label: &str) {
    let mut runs = Vec::new();
    for _ in 0..RUNS {
        runs.push(run_once(git, repo).await);
    }
    runs.sort_by_key(|r| r.total);
    let median = &runs[RUNS / 2];
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    println!(
        "{label}: {} commits, max column {} | first page {:.0} ms | all {:.0} ms | JSON {:.1} MB in {:.0} ms   (median of {RUNS}; best total {:.0} ms)",
        median.commits,
        median.max_column,
        ms(median.first_page),
        ms(median.total),
        median.json_bytes as f64 / 1_000_000.0,
        ms(median.json),
        ms(runs[0].total),
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "benchmark: run with `npm run bench:graph`"]
async fn bench_graph_loading() {
    let git = gitcraft_core::testing::isolated_git();
    let commits: usize = std::env::var("GITCRAFT_BENCH_COMMITS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);

    let started = Instant::now();
    let repo = TestRepo::synthetic(commits).await;
    println!(
        "git {} — generated {commits} synthetic commits in {:.1} s",
        git.version().await.expect("version"),
        started.elapsed().as_secs_f64()
    );

    measure(&git, repo.path(), "synthetic, no commit-graph  ").await;
    repo.run(&["commit-graph", "write", "--reachable"]).await;
    measure(&git, repo.path(), "synthetic, with commit-graph").await;

    if let Some(real) = std::env::var_os("GITCRAFT_BENCH_REPO") {
        let real = Path::new(&real);
        measure(&git, real, &format!("{}", real.display())).await;
    }
}
