//! Fetch, pull and push against local bare remotes (never a real server).

use gitcraft_core::ops::branch::BlockingFile;
use gitcraft_core::ops::refs::read_refs;
use gitcraft_core::ops::remote::{
    PullBlock, PullMode, PullOutcome, abort_in_progress, configured_pull_mode, fetch, pull,
    stash_and_pull,
};
use gitcraft_core::ops::status::working_status;
use gitcraft_core::parse::refs::{GitRef, RefKind};
use gitcraft_core::parse::status::InProgress;
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

async fn pull_now(repo: &TestRepo, mode: PullMode, allow_conflicts: bool) -> PullOutcome {
    pull(&git(), repo.path(), mode, allow_conflicts, &c(), |_| {})
        .await
        .expect("pull")
}

fn read(repo: &TestRepo, path: &str) -> String {
    std::fs::read_to_string(repo.path().join(path)).expect("read")
}

#[tokio::test]
async fn pulls_fast_forward_merge_and_rebase() {
    let (repo, other, _remote) = sandbox().await;
    let pushed = other_pushes(&other, "nuevo.txt", "hola\n").await;
    assert_eq!(
        pull_now(&repo, PullMode::Merge, false).await,
        PullOutcome::Pulled {
            upstream: "origin/main".into(),
            commits: 1,
            mode: PullMode::Merge,
            fast_forward: true
        }
    );
    assert_eq!(repo.head().await, pushed);
    assert_eq!(
        pull_now(&repo, PullMode::Merge, false).await,
        PullOutcome::UpToDate {
            upstream: "origin/main".into()
        }
    );

    // Diverged, different files: a merge commit…
    other_pushes(&other, "otra.txt", "otra\n").await;
    repo.commit_file("mio.txt", "mio\n", "local").await;
    let outcome = pull_now(&repo, PullMode::Merge, false).await;
    assert!(matches!(
        outcome,
        PullOutcome::Pulled {
            fast_forward: false,
            ..
        }
    ));
    assert_eq!(
        repo.run(&["rev-list", "--parents", "-n1", "HEAD"])
            .await
            .split(' ')
            .count(),
        3
    );

    // …or, with rebase, the local commit on top.
    let theirs = other_pushes(&other, "otra2.txt", "otra\n").await;
    repo.commit_file("mio2.txt", "mio\n", "local 2").await;
    let outcome = pull_now(&repo, PullMode::Rebase, false).await;
    assert!(matches!(
        outcome,
        PullOutcome::Pulled {
            mode: PullMode::Rebase,
            ..
        }
    ));
    // Local commits replayed on top: linear after theirs.
    repo.run(&["merge-base", "--is-ancestor", &theirs, "HEAD"])
        .await;
    assert_eq!(
        repo.run(&["rev-list", "--merges", &format!("{theirs}..HEAD")])
            .await,
        ""
    );
    assert_eq!(read(&repo, "mio2.txt"), "mio\n");
}

#[tokio::test]
async fn never_overwrites_uncommitted_changes() {
    let (repo, other, _remote) = sandbox().await;
    other_pushes(&other, "lista.txt", "uno\nDOS\n").await;
    repo.write_file("lista.txt", "uno\ndos\nmio\n");
    repo.write_file("aparte.txt", "sin seguimiento\n");
    let head = repo.head().await;

    let outcome = pull_now(&repo, PullMode::Merge, false).await;
    assert_eq!(
        outcome,
        PullOutcome::Blocked {
            files: vec![BlockingFile {
                path: "lista.txt".into(),
                untracked: false
            }],
            reason: PullBlock::Overwrite
        }
    );
    assert_eq!(repo.head().await, head);
    assert_eq!(read(&repo, "lista.txt"), "uno\ndos\nmio\n");

    // "Save my changes, then pull".
    let outcome = stash_and_pull(&git(), repo.path(), PullMode::Merge, false, &c(), |_| {})
        .await
        .expect("stash and pull");
    assert!(matches!(outcome, PullOutcome::Pulled { .. }));
    assert_eq!(read(&repo, "lista.txt"), "uno\nDOS\n");
    assert_eq!(
        repo.run(&["stash", "list", "--format=%gs"]).await,
        "On main: GitCRAFT: antes de hacer pull"
    );
    assert_eq!(
        repo.run(&["show", "stash@{0}:lista.txt"]).await,
        "uno\ndos\nmio"
    );
    assert_eq!(
        repo.run(&["show", "stash@{0}^3:aparte.txt"]).await,
        "sin seguimiento"
    );
}

#[tokio::test]
async fn untracked_files_in_the_way_and_rebase_with_changes_block() {
    let (repo, other, _remote) = sandbox().await;
    other_pushes(&other, "nuevo.txt", "de otra persona\n").await;
    repo.write_file("nuevo.txt", "mio, sin seguimiento\n");
    assert_eq!(
        pull_now(&repo, PullMode::Merge, false).await,
        PullOutcome::Blocked {
            files: vec![BlockingFile {
                path: "nuevo.txt".into(),
                untracked: true
            }],
            reason: PullBlock::Overwrite
        }
    );
    std::fs::remove_file(repo.path().join("nuevo.txt")).expect("rm");

    // A change in an unrelated file: fine for a merge, not for a rebase.
    repo.write_file("lista.txt", "uno\ndos\ntres\n");
    assert!(matches!(
        pull_now(&repo, PullMode::Rebase, false).await,
        PullOutcome::Blocked {
            reason: PullBlock::RebaseNeedsClean,
            ..
        }
    ));
    assert!(matches!(
        pull_now(&repo, PullMode::Merge, false).await,
        PullOutcome::Pulled { .. }
    ));
    assert_eq!(read(&repo, "lista.txt"), "uno\ndos\ntres\n", "kept");
}

#[tokio::test]
async fn reports_conflicts_first_and_can_abort_them() {
    for mode in [PullMode::Merge, PullMode::Rebase] {
        let (repo, other, _remote) = sandbox().await;
        other_pushes(&other, "lista.txt", "uno\nDE OTRA\n").await;
        repo.commit_file("lista.txt", "uno\nMIO\n", "local").await;
        let head = repo.head().await;

        let outcome = pull_now(&repo, mode, false).await;
        let PullOutcome::WouldConflict { files, exact, .. } = outcome else {
            panic!("{mode:?}: {outcome:?}");
        };
        assert_eq!(files, ["lista.txt"]);
        assert_eq!(exact, mode == PullMode::Merge);
        assert_eq!(repo.head().await, head, "nothing touched");
        assert_eq!(repo.run(&["status", "--porcelain"]).await, "");

        assert_eq!(
            pull_now(&repo, mode, true).await,
            PullOutcome::Conflicted {
                files: vec!["lista.txt".into()],
                mode
            }
        );
        let status = working_status(&git(), repo.path(), &c())
            .await
            .expect("status");
        let expected = match mode {
            PullMode::Merge => InProgress::Merge,
            PullMode::Rebase => InProgress::Rebase,
        };
        assert_eq!(status.in_progress, Some(expected));

        abort_in_progress(&git(), repo.path(), &c())
            .await
            .expect("abort");
        assert_eq!(repo.head().await, head);
        assert_eq!(read(&repo, "lista.txt"), "uno\nMIO\n");
        let status = working_status(&git(), repo.path(), &c())
            .await
            .expect("status");
        assert_eq!(status.in_progress, None);
        assert!(status.is_clean());
    }
}

#[tokio::test]
async fn explains_branches_without_a_usable_upstream() {
    let (repo, other, _remote) = sandbox().await;
    repo.checkout("feature").await;
    assert_eq!(
        pull_now(&repo, PullMode::Merge, false).await,
        PullOutcome::NoUpstream {
            branch: "feature".into()
        }
    );

    repo.run(&["push", "--quiet", "--set-upstream", "origin", "feature"])
        .await;
    other
        .run(&["push", "--quiet", "origin", "--delete", "feature"])
        .await;
    repo.run(&["fetch", "--quiet", "--prune"]).await;
    assert_eq!(
        pull_now(&repo, PullMode::Merge, false).await,
        PullOutcome::UpstreamGone {
            upstream: "origin/feature".into()
        }
    );

    repo.run(&["switch", "--quiet", "--detach", "main"]).await;
    let err = pull(&git(), repo.path(), PullMode::Merge, false, &c(), |_| {})
        .await
        .expect_err("detached");
    assert_eq!(err.kind(), ErrorKind::DetachedHead);
}

#[tokio::test]
async fn reads_the_configured_pull_mode() {
    let (repo, _other, _remote) = sandbox().await;
    let mode = |branch: Option<&'static str>| {
        let repo = &repo;
        async move {
            configured_pull_mode(&git(), repo.path(), branch, &c())
                .await
                .expect("mode")
        }
    };
    assert_eq!(mode(Some("main")).await, PullMode::Merge);
    repo.run(&["config", "pull.rebase", "true"]).await;
    assert_eq!(mode(Some("main")).await, PullMode::Rebase);
    repo.run(&["config", "branch.main.rebase", "false"]).await;
    assert_eq!(mode(Some("main")).await, PullMode::Merge);
    assert_eq!(mode(None).await, PullMode::Rebase);
}
