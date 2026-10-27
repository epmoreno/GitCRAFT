//! Commit log parsing against real git output.

use std::path::PathBuf;

use gitcraft_core::ops::log::{graph_log_args, stream_commits};
use gitcraft_core::parse::log::{Commit, parse_commit};
use gitcraft_core::testing::{AUTHOR_NAME, TestRepo};
use gitcraft_core::{CancellationToken, Git};

fn git() -> Git {
    Git::discover().expect("git must be installed to run the tests")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/log_edge_cases.z")
}

/// History with the awkward cases: unicode author, quotes and pipes in the
/// subject, an empty subject, a multi-line message and an octopus merge.
async fn edge_case_repo() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_with(&["-m", "root"]).await;
    repo.create_branch("a").await;
    repo.create_branch("b").await;

    repo.checkout("a").await;
    repo.commit_with(&[
        "--author=José Ñúñez <jose@example.com>",
        "-m",
        "fix: \"quotes\" | pipes & <tags> ñ",
    ])
    .await;

    repo.checkout("b").await;
    repo.commit_with(&["--allow-empty-message", "-m", ""]).await;

    repo.checkout("main").await;
    repo.commit_with(&["-m", "Subject line\n\nBody that must not appear"])
        .await;
    repo.run(&["merge", "--no-ff", "--quiet", "-m", "octopus", "a", "b"])
        .await;
    repo
}

async fn raw_log(repo: &TestRepo) -> Vec<u8> {
    repo.read()
        .args(graph_log_args(false))
        .output(&CancellationToken::new())
        .await
        .expect("git log")
        .stdout
}

fn parse_all(raw: &[u8]) -> Vec<Commit> {
    raw.split(|&b| b == 0)
        .filter(|r| !r.is_empty())
        .map(|r| parse_commit(r).expect("parse record"))
        .collect()
}

/// The checked-in fixture is real git output; regenerate it with
/// `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test log`.
#[tokio::test]
async fn fixture_matches_current_git_output() {
    let repo = edge_case_repo().await;
    let fresh = raw_log(&repo).await;
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(fixture_path().parent().expect("dir")).expect("mkdir");
        std::fs::write(fixture_path(), &fresh).expect("write fixture");
    }
    let fixture = std::fs::read(fixture_path()).expect("read fixture");
    assert!(
        fixture == fresh,
        "git log output changed; see UPDATE_FIXTURES"
    );
}

#[test]
fn parses_the_edge_case_fixture() {
    let raw = std::fs::read(fixture_path()).expect("read fixture");
    assert_eq!(raw.last(), Some(&0), "-z terminates every record with NUL");
    let commits = parse_all(&raw);
    assert_eq!(commits.len(), 5);

    let octopus = &commits[0];
    assert_eq!(octopus.summary, "octopus");
    assert_eq!(octopus.parents.len(), 3);

    let by_summary = |s: &str| commits.iter().find(|c| c.summary == s);
    let unicode = by_summary("fix: \"quotes\" | pipes & <tags> ñ").expect("unicode commit");
    assert_eq!(unicode.author_name, "José Ñúñez");
    assert_eq!(unicode.author_email, "jose@example.com");

    assert!(by_summary("").is_some(), "empty subject");
    assert!(
        by_summary("Subject line").is_some(),
        "only the subject line"
    );
    assert!(commits.iter().all(|c| !c.summary.contains("Body")));

    let root = commits.last().expect("root");
    assert_eq!(root.summary, "root");
    assert!(root.parents.is_empty());
    assert_eq!(root.author_name, AUTHOR_NAME);

    // Topological order: every parent appears after its children.
    for (i, commit) in commits.iter().enumerate() {
        for parent in &commit.parents {
            let at = commits
                .iter()
                .position(|c| &c.oid == parent)
                .expect("parent listed");
            assert!(at > i, "{} listed before its child", parent);
        }
    }
}

#[tokio::test]
async fn streams_the_sample_history_in_topological_order() {
    let (repo, ids) = TestRepo::sample_history().await;
    let mut commits = Vec::new();
    stream_commits(&git(), repo.path(), false, &CancellationToken::new(), |c| {
        commits.push(c)
    })
    .await
    .expect("stream");

    let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries.len(), 7);
    assert_eq!(summaries.first(), Some(&"E"));
    assert_eq!(summaries.last(), Some(&"A"));
    // --topo-order keeps each line of history together.
    let pos = |s: &str| summaries.iter().position(|x| *x == s).expect(s);
    assert!(pos("D") < pos("C") && pos("C") < pos("A"));
    assert!(pos("Merge branch 'feature'") < pos("B") && pos("Merge branch 'feature'") < pos("D"));

    let merge = &commits[pos("Merge branch 'feature'")];
    assert_eq!(merge.oid, ids["M"]);
    assert_eq!(merge.parents, [ids["B"].clone(), ids["D"].clone()]);
    let topic = &commits[pos("F")];
    assert_eq!(topic.oid, ids["F"]);
    assert_eq!(topic.parents, [ids["A"].clone()]);
}

#[tokio::test]
async fn an_empty_repository_has_no_commits() {
    let repo = TestRepo::new().await;
    let mut count = 0;
    stream_commits(
        &git(),
        repo.path(),
        false,
        &CancellationToken::new(),
        |_| count += 1,
    )
    .await
    .expect("stream");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn a_detached_head_is_included_only_when_asked() {
    let (repo, _) = TestRepo::sample_history().await;
    repo.run(&["switch", "--quiet", "--detach", "HEAD~1"]).await;
    let detached = repo.commit_with(&["-m", "detached work"]).await;

    let collect = |include_head| {
        let repo = &repo;
        async move {
            let mut oids = Vec::new();
            stream_commits(
                &git(),
                repo.path(),
                include_head,
                &CancellationToken::new(),
                |c| oids.push(c.oid),
            )
            .await
            .expect("stream");
            oids
        }
    };
    assert!(!collect(false).await.contains(&detached));
    assert!(collect(true).await.contains(&detached));
}
