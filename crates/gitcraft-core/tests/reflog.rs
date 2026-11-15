//! The history of a branch or of HEAD (task 11.5), against real
//! repositories and a fixture of git's real output.

use std::path::PathBuf;

use gitcraft_core::CancellationToken;
use gitcraft_core::ops::branch::create_branch;
use gitcraft_core::ops::reflog::{Expiry, ReflogExpiry, reflog};
use gitcraft_core::parse::reflog::{REFLOG_FORMAT, parse_reflog};
use gitcraft_core::testing::TestRepo;

fn c() -> CancellationToken {
    CancellationToken::new()
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reflog.z")
}

/// main: A, B, then a hard reset back to A and a commit C whose subject has
/// a colon; HEAD also went to `side` and back.
async fn repo_with_history() -> (TestRepo, [String; 3]) {
    let repo = TestRepo::new().await;
    let a = repo.commit_file("a.txt", "a\n", "A").await;
    let b = repo.commit_file("b.txt", "b\n", "B").await;
    repo.run(&["reset", "--quiet", "--hard", &a]).await;
    let c = repo.commit_file("c.txt", "c\n", "C: con dos puntos").await;
    repo.run(&["switch", "--quiet", "-c", "side"]).await;
    repo.checkout("main").await;
    (repo, [a, b, c])
}

/// Regenerate with `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test reflog`.
#[tokio::test]
async fn fixture_matches_current_git_output() {
    let (repo, _) = repo_with_history().await;
    let fresh = repo
        .read()
        .args([
            "log",
            "-g",
            "-z",
            "--date=unix",
            REFLOG_FORMAT,
            "refs/heads/main",
            "--",
        ])
        .output(&c())
        .await
        .expect("log -g")
        .stdout_lossy();
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(fixture_path(), &fresh).expect("write fixture");
    }
    let fixture = std::fs::read_to_string(fixture_path()).expect("read fixture");
    assert!(
        fixture == fresh,
        "reflog output changed; see UPDATE_FIXTURES"
    );
}

#[test]
fn parses_the_reflog_fixture() {
    let raw = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let entries = parse_reflog(&raw).expect("parse");
    let summary: Vec<_> = entries
        .iter()
        .map(|e| (e.index, e.subject.as_str()))
        .collect();
    assert_eq!(
        summary,
        [(0, "C: con dos puntos"), (1, "A"), (2, "B"), (3, "A")]
    );
    assert!(entries[1].message.starts_with("reset: moving to"));
    assert!(entries.iter().all(|e| e.oid.len() == 40 && e.time > 0));
    // Newest first.
    assert!(entries.windows(2).all(|w| w[0].time >= w[1].time));
}

#[tokio::test]
async fn a_commit_lost_in_a_hard_reset_is_found_and_rescued() {
    let (repo, [a, b, commit_c]) = repo_with_history().await;
    let log = reflog(repo.git(), repo.path(), "refs/heads/main", &c())
        .await
        .expect("reflog");
    let oids: Vec<_> = log.entries.iter().map(|e| e.oid.as_str()).collect();
    assert_eq!(
        oids,
        [commit_c.as_str(), a.as_str(), b.as_str(), a.as_str()]
    );
    assert!(!log.more);
    let lonely: Vec<_> = log.entries.iter().map(|e| e.only_here).collect();
    assert_eq!(lonely, [false, false, true, false], "only B is lost");

    // "Create a branch here": nothing else moves.
    create_branch(repo.git(), repo.path(), "rescate", &b, false, &c())
        .await
        .expect("branch");
    assert_eq!(repo.rev_parse("rescate").await, b);
    assert_eq!(repo.rev_parse("main").await, commit_c);
    let log = reflog(repo.git(), repo.path(), "refs/heads/main", &c())
        .await
        .expect("reflog");
    assert!(
        log.entries.iter().all(|e| !e.only_here),
        "B has a branch now"
    );
}

#[tokio::test]
async fn says_how_long_git_keeps_the_entries() {
    let (repo, _) = repo_with_history().await;
    let cancel = c();
    let read = || reflog(repo.git(), repo.path(), "refs/heads/main", &cancel);
    let defaults = read().await.expect("reflog").expiry;
    assert_eq!(
        defaults,
        ReflogExpiry {
            reachable: Expiry::Days { days: 90 },
            unreachable: Expiry::Days { days: 30 },
        }
    );
    repo.run(&["config", "gc.reflogExpireUnreachable", "2.weeks.ago"])
        .await;
    repo.run(&["config", "gc.reflogExpire", "never"]).await;
    let configured = read().await.expect("reflog").expiry;
    assert_eq!(configured.unreachable, Expiry::Days { days: 14 });
    assert_eq!(configured.reachable, Expiry::Never);
}

#[tokio::test]
async fn head_has_its_own_history_with_the_switches() {
    let (repo, _) = repo_with_history().await;
    let log = reflog(repo.git(), repo.path(), "HEAD", &c())
        .await
        .expect("reflog");
    assert!(
        log.entries[0]
            .message
            .starts_with("checkout: moving from side to main")
    );
    assert!(log.entries.len() > 4);
}

#[tokio::test]
async fn a_branch_deleted_and_created_again_starts_a_new_history() {
    let (repo, _) = repo_with_history().await;
    repo.run(&["branch", "-D", "side"]).await;
    let gone = reflog(repo.git(), repo.path(), "refs/heads/side", &c())
        .await
        .expect("reflog");
    assert!(
        gone.entries.is_empty(),
        "git drops a deleted branch's reflog"
    );
    repo.run(&["branch", "side"]).await;
    let again = reflog(repo.git(), repo.path(), "refs/heads/side", &c())
        .await
        .expect("reflog");
    assert_eq!(again.entries.len(), 1);
}

#[tokio::test]
async fn only_branches_and_head_are_read() {
    let (repo, _) = repo_with_history().await;
    for refname in ["refs/tags/v1", "--all", "main"] {
        assert!(
            reflog(repo.git(), repo.path(), refname, &c())
                .await
                .is_err(),
            "{refname}"
        );
    }
}
