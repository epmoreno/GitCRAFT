//! Fetch, pull and push against local bare remotes (never a real server).

use gitcraft_core::ops::refs::read_refs;
use gitcraft_core::ops::remote::fetch;
use gitcraft_core::parse::refs::{GitRef, RefKind};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind, Git};

fn git() -> Git {
    Git::discover().expect("git must be installed to run the tests")
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

/// A repository with `origin` (a bare remote, kept alive by the returned
/// guard) and a second clone of it: "someone else".
async fn sandbox() -> (TestRepo, TestRepo, tempfile::TempDir) {
    let repo = TestRepo::new().await;
    repo.commit_file("lista.txt", "uno\ndos\n", "base").await;
    repo.create_branch("feature").await;
    let remote = repo.add_bare_remote("origin").await;
    repo.run(&["branch", "--quiet", "--set-upstream-to=origin/main", "main"])
        .await;
    let other = TestRepo::clone_of(remote.path()).await;
    (repo, other, remote)
}

async fn other_pushes(other: &TestRepo, file: &str, text: &str) -> String {
    let head = other.commit_file(file, text, "otra persona").await;
    other.run(&["push", "--quiet", "origin", "main"]).await;
    head
}

#[tokio::test]
async fn fetches_without_touching_local_branches() {
    let (repo, other, _remote) = sandbox().await;
    let local = repo.head().await;
    let pushed = other_pushes(&other, "nuevo.txt", "hola\n").await;

    let mut progress = Vec::new();
    fetch(&git(), repo.path(), None, false, &c(), |line| {
        progress.push(line.to_owned())
    })
    .await
    .expect("fetch");
    assert_eq!(repo.rev_parse("origin/main").await, pushed);
    assert_eq!(repo.head().await, local, "main is not moved");
    assert!(!repo.path().join("nuevo.txt").exists());
}

#[tokio::test]
async fn prunes_only_when_asked() {
    let (repo, other, _remote) = sandbox().await;
    repo.run(&["push", "--quiet", "origin", "feature"]).await;
    fetch(&git(), repo.path(), Some("origin"), false, &c(), |_| {})
        .await
        .expect("fetch");
    other
        .run(&["push", "--quiet", "origin", "--delete", "feature"])
        .await;

    fetch(&git(), repo.path(), Some("origin"), false, &c(), |_| {})
        .await
        .expect("fetch");
    assert!(repo.run(&["branch", "-r"]).await.contains("origin/feature"));
    fetch(&git(), repo.path(), Some("origin"), true, &c(), |_| {})
        .await
        .expect("fetch --prune");
    assert!(!repo.run(&["branch", "-r"]).await.contains("origin/feature"));
    assert!(
        repo.run(&["branch"]).await.contains("feature"),
        "the local branch stays"
    );
}

#[tokio::test]
async fn refuses_unknown_remotes_and_explains_unreachable_ones() {
    let (repo, _other, _remote) = sandbox().await;
    for name in ["nope", "--upload-pack=touch /tmp/x"] {
        assert!(
            fetch(&git(), repo.path(), Some(name), false, &c(), |_| {})
                .await
                .is_err(),
            "{name}"
        );
    }

    let missing = tempfile::tempdir().expect("tempdir");
    let gone = missing.path().join("gone.git");
    repo.run(&["remote", "add", "lost", gone.to_str().expect("utf-8")])
        .await;
    let err = fetch(&git(), repo.path(), Some("lost"), false, &c(), |_| {})
        .await
        .expect_err("missing remote");
    assert_eq!(err.kind(), ErrorKind::RemoteUnreachable);
}

#[tokio::test]
async fn can_be_cancelled() {
    let (repo, _other, _remote) = sandbox().await;
    let cancel = c();
    cancel.cancel();
    let err = fetch(&git(), repo.path(), None, false, &cancel, |_| {})
        .await
        .expect_err("cancelled");
    assert_eq!(err.kind(), ErrorKind::Cancelled);
}

async fn local(repo: &TestRepo, name: &str) -> GitRef {
    read_refs(&git(), repo.path(), &c())
        .await
        .expect("refs")
        .refs
        .into_iter()
        .find(|r| r.kind == RefKind::Local && r.short_name == name)
        .expect("branch")
}

#[tokio::test]
async fn counts_commits_ahead_and_behind_the_upstream() {
    let (repo, other, _remote) = sandbox().await;
    let main = local(&repo, "main").await;
    assert_eq!((main.ahead, main.behind), (Some(0), Some(0)));
    let feature = local(&repo, "feature").await;
    assert_eq!(
        (feature.ahead, feature.behind, feature.upstream_gone),
        (None, None, false)
    );

    other_pushes(&other, "a.txt", "a\n").await;
    other_pushes(&other, "b.txt", "b\n").await;
    repo.commit_file("mio.txt", "mio\n", "local").await;
    let main = local(&repo, "main").await;
    assert_eq!(
        (main.ahead, main.behind),
        (Some(1), Some(0)),
        "not fetched yet"
    );

    fetch(&git(), repo.path(), None, false, &c(), |_| {})
        .await
        .expect("fetch");
    let main = local(&repo, "main").await;
    assert_eq!((main.ahead, main.behind), (Some(1), Some(2)));

    // Upstream deleted on the remote and pruned: "gone".
    repo.run(&["push", "--quiet", "--set-upstream", "origin", "feature"])
        .await;
    other
        .run(&["push", "--quiet", "origin", "--delete", "feature"])
        .await;
    fetch(&git(), repo.path(), None, true, &c(), |_| {})
        .await
        .expect("fetch --prune");
    let feature = local(&repo, "feature").await;
    assert!(feature.upstream_gone);
    assert_eq!((feature.ahead, feature.behind), (None, None));
}
