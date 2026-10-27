//! Reading refs and HEAD from real repositories.

use std::path::PathBuf;

use gitcraft_core::ops::refs::read_refs;
use gitcraft_core::parse::refs::{GitRef, Head, REF_PATTERNS, REFS_FORMAT, RefKind, parse_refs};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, Git};

fn git() -> Git {
    Git::discover().expect("git must be installed to run the tests")
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/refs_sample.txt")
}

fn find<'a>(refs: &'a [GitRef], short: &str, kind: RefKind) -> &'a GitRef {
    refs.iter()
        .find(|r| r.short_name == short && r.kind == kind)
        .unwrap_or_else(|| panic!("{short} ({kind:?}) missing"))
}

/// sample_history() plus remotes `origin` and `team/fork` (a name with a
/// slash), an `origin/HEAD` symref and `main` tracking `origin/main`.
async fn repo_with_remotes() -> (TestRepo, Vec<tempfile::TempDir>) {
    let (repo, _) = TestRepo::sample_history().await;
    let origin = repo.add_bare_remote("origin").await;
    let fork = repo.add_bare_remote("team/fork").await;
    repo.run(&["remote", "set-head", "origin", "main"]).await;
    repo.run(&["branch", "--quiet", "--set-upstream-to=origin/main", "main"])
        .await;
    (repo, vec![origin, fork])
}

/// Regenerate with `UPDATE_FIXTURES=1 cargo test -p gitcraft-core --test refs`.
#[tokio::test]
async fn fixture_matches_current_git_output() {
    let (repo, _remotes) = repo_with_remotes().await;
    let fresh = repo
        .read()
        .args(["for-each-ref", REFS_FORMAT])
        .args(REF_PATTERNS)
        .output(&CancellationToken::new())
        .await
        .expect("for-each-ref")
        .stdout_lossy();
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(fixture_path(), &fresh).expect("write fixture");
    }
    let fixture = std::fs::read_to_string(fixture_path()).expect("read fixture");
    assert!(
        fixture == fresh,
        "for-each-ref output changed; see UPDATE_FIXTURES"
    );
}

#[test]
fn parses_the_refs_fixture() {
    let raw = std::fs::read_to_string(fixture_path()).expect("read fixture");
    let remotes = vec!["origin".to_string(), "team/fork".to_string()];
    let refs = parse_refs(&raw, &remotes).expect("parse");

    let main = find(&refs, "main", RefKind::Local);
    assert!(main.is_head);
    assert_eq!(main.upstream.as_deref(), Some("origin/main"));
    assert_eq!(
        find(&refs, "origin/main", RefKind::Remote).target,
        main.target
    );
    assert_eq!(
        find(&refs, "team/fork/topic", RefKind::Remote)
            .remote
            .as_deref(),
        Some("team/fork")
    );
    assert!(
        refs.iter().all(|r| r.short_name != "origin/HEAD"),
        "symbolic refs are skipped"
    );
    // 3 local + 3 per remote + 2 tags.
    assert_eq!(refs.len(), 3 + 3 + 3 + 2);
}

#[tokio::test]
async fn reads_the_sample_history() {
    let (repo, ids) = TestRepo::sample_history().await;
    let refs = read_refs(&git(), repo.path(), &CancellationToken::new())
        .await
        .expect("refs");

    assert_eq!(
        refs.head,
        Head::Branch {
            name: "main".into(),
            oid: Some(ids["E"].clone())
        }
    );
    assert!(!refs.head_is_detached());
    assert!(refs.remotes.is_empty());
    assert_eq!(find(&refs.refs, "feature", RefKind::Local).target, ids["D"]);
    assert_eq!(find(&refs.refs, "topic", RefKind::Local).target, ids["F"]);
    // Annotated tag peeled to its commit; lightweight tag points directly.
    assert_eq!(find(&refs.refs, "v1.0", RefKind::Tag).target, ids["B"]);
    assert_eq!(find(&refs.refs, "v2.0", RefKind::Tag).target, ids["E"]);
}

#[tokio::test]
async fn reads_remotes_and_tracking() {
    let (repo, _remotes) = repo_with_remotes().await;
    let refs = read_refs(&git(), repo.path(), &CancellationToken::new())
        .await
        .expect("refs");
    assert_eq!(refs.remotes, ["origin", "team/fork"]);
    let main = find(&refs.refs, "main", RefKind::Local);
    assert_eq!(main.upstream.as_deref(), Some("origin/main"));
}

#[tokio::test]
async fn reads_a_detached_head() {
    let (repo, ids) = TestRepo::sample_history().await;
    repo.run(&["switch", "--quiet", "--detach", "v1.0"]).await;
    let refs = read_refs(&git(), repo.path(), &CancellationToken::new())
        .await
        .expect("refs");
    assert_eq!(
        refs.head,
        Head::Detached {
            oid: ids["B"].clone()
        }
    );
    assert!(refs.head_is_detached());
    assert!(refs.refs.iter().all(|r| !r.is_head));
}

#[tokio::test]
async fn reads_an_empty_repository() {
    let repo = TestRepo::new().await;
    let refs = read_refs(&git(), repo.path(), &CancellationToken::new())
        .await
        .expect("refs");
    assert_eq!(
        refs.head,
        Head::Branch {
            name: "main".into(),
            oid: None
        }
    );
    assert!(refs.refs.is_empty());
}
