//! Creating, switching, renaming and deleting branches against real repos.

use gitcraft_core::ops::branch::{
    BlockingFile, BranchNameCheck, CheckoutOutcome, check_branch_name, checkout_branch,
    create_branch, delete_branch, preview_branch_deletion, rename_branch, stash_and_checkout,
};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::{CancellationToken, ErrorKind, Git};

fn git() -> Git {
    Git::discover().expect("git must be installed to run the tests")
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

async fn current_branch(repo: &TestRepo) -> String {
    repo.run(&["symbolic-ref", "--short", "HEAD"]).await
}

async fn branches(repo: &TestRepo) -> Vec<String> {
    repo.run(&["for-each-ref", "--format=%(refname:short)", "refs/heads"])
        .await
        .lines()
        .map(str::to_owned)
        .collect()
}

/// main: base ← two; feature (from base): f1 ← f2, changing `shared.txt`.
async fn repo_with_feature() -> TestRepo {
    let repo = TestRepo::new().await;
    repo.commit_file("shared.txt", "base\n", "base").await;
    repo.create_branch("feature").await;
    repo.commit_file("main.txt", "main\n", "two").await;
    repo.checkout("feature").await;
    repo.commit_file("shared.txt", "feature 1\n", "f1").await;
    repo.commit_file("shared.txt", "feature 2\n", "f2").await;
    repo.checkout("main").await;
    repo
}

#[tokio::test]
async fn checks_branch_names() {
    let repo = repo_with_feature().await;
    let check = |name: &'static str| {
        let repo = &repo;
        async move {
            check_branch_name(&git(), repo.path(), name, &c())
                .await
                .expect("check")
        }
    };
    assert_eq!(check("nueva/rama-ñ").await, BranchNameCheck::Valid);
    assert_eq!(check("feature").await, BranchNameCheck::Exists);
    for invalid in [
        "",
        "-x",
        "--force",
        "a..b",
        "con espacio",
        "HEAD",
        "@{-1}",
        "a.lock",
    ] {
        assert_eq!(
            check(invalid).await,
            BranchNameCheck::Invalid,
            "{invalid:?}"
        );
    }
}

#[tokio::test]
async fn creates_branches_at_a_commit_or_tracking_a_remote() {
    let repo = repo_with_feature().await;
    let base = repo.rev_parse("main~1").await;
    create_branch(&git(), repo.path(), "at-base", &base, false, &c())
        .await
        .expect("create");
    assert_eq!(repo.rev_parse("at-base").await, base);
    assert_eq!(
        current_branch(&repo).await,
        "main",
        "creating does not switch"
    );

    let _remote = repo.add_bare_remote("origin").await;
    create_branch(
        &git(),
        repo.path(),
        "tracked",
        "refs/remotes/origin/feature",
        true,
        &c(),
    )
    .await
    .expect("create tracking");
    assert_eq!(
        repo.run(&["rev-parse", "--abbrev-ref", "tracked@{upstream}"])
            .await,
        "origin/feature"
    );

    let err = create_branch(&git(), repo.path(), "feature", &base, false, &c())
        .await
        .expect_err("exists");
    assert_eq!(err.kind(), ErrorKind::BranchExists);
    let err = create_branch(&git(), repo.path(), "-D", &base, false, &c())
        .await
        .expect_err("invalid");
    assert_eq!(err.kind(), ErrorKind::InvalidBranchName);
    let err = create_branch(&git(), repo.path(), "ok", "--orphan", false, &c())
        .await
        .expect_err("option as start point");
    assert_eq!(err.kind(), ErrorKind::Unknown);
    assert!(!branches(&repo).await.contains(&"ok".to_owned()));
}

#[tokio::test]
async fn renames_branches_including_the_current_one() {
    let repo = repo_with_feature().await;
    let tip = repo.rev_parse("main").await;
    rename_branch(&git(), repo.path(), "main", "principal", &c())
        .await
        .expect("rename");
    assert_eq!(current_branch(&repo).await, "principal");
    assert_eq!(repo.rev_parse("principal").await, tip);

    let err = rename_branch(&git(), repo.path(), "principal", "feature", &c())
        .await
        .expect_err("taken");
    assert_eq!(err.kind(), ErrorKind::BranchExists);
    let err = rename_branch(&git(), repo.path(), "gone", "other", &c())
        .await
        .expect_err("missing");
    assert_eq!(err.kind(), ErrorKind::BranchChanged);
    assert_eq!(branches(&repo).await, ["feature", "principal"]);
}

#[tokio::test]
async fn previews_which_commits_a_deletion_leaves_without_a_branch() {
    let repo = repo_with_feature().await;
    let preview = preview_branch_deletion(&git(), repo.path(), "feature", &c())
        .await
        .expect("preview");
    assert_eq!(preview.tip, repo.rev_parse("feature").await);
    assert_eq!(preview.orphaned, 2);
    let summaries: Vec<_> = preview.commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["f2", "f1"]);
    assert_eq!(preview.upstream, None);

    // A tag keeps them reachable.
    repo.run(&["tag", "keep", "feature"]).await;
    let preview = preview_branch_deletion(&git(), repo.path(), "feature", &c())
        .await
        .expect("preview");
    assert_eq!(preview.orphaned, 0);
    assert!(preview.commits.is_empty());

    // So does another branch, and a pushed copy.
    repo.run(&["tag", "--delete", "keep"]).await;
    let _remote = repo.add_bare_remote("origin").await;
    repo.run(&["branch", "--set-upstream-to=origin/feature", "feature"])
        .await;
    let preview = preview_branch_deletion(&git(), repo.path(), "feature", &c())
        .await
        .expect("preview");
    assert_eq!(preview.orphaned, 0);
    assert_eq!(preview.upstream.as_deref(), Some("origin/feature"));
}

#[tokio::test]
async fn deletes_only_what_was_confirmed() {
    let repo = repo_with_feature().await;
    let preview = preview_branch_deletion(&git(), repo.path(), "feature", &c())
        .await
        .expect("preview");

    // The branch moved after the confirmation was shown: nothing happens.
    repo.checkout("feature").await;
    repo.commit_file("late.txt", "late\n", "late").await;
    repo.checkout("main").await;
    let err = delete_branch(&git(), repo.path(), "feature", &preview.tip, &c())
        .await
        .expect_err("moved");
    assert_eq!(err.kind(), ErrorKind::BranchChanged);
    assert!(branches(&repo).await.contains(&"feature".to_owned()));

    // Confirmed with the current tip: deleted although not merged.
    let preview = preview_branch_deletion(&git(), repo.path(), "feature", &c())
        .await
        .expect("preview");
    assert_eq!(preview.orphaned, 3);
    delete_branch(&git(), repo.path(), "feature", &preview.tip, &c())
        .await
        .expect("delete");
    assert_eq!(branches(&repo).await, ["main"]);
    // Recoverable with the tip shown in the confirmation.
    repo.run(&["branch", "feature", &preview.tip]).await;
    assert_eq!(repo.rev_parse("feature").await, preview.tip);

    // The current branch cannot be deleted.
    let tip = repo.rev_parse("main").await;
    assert!(
        delete_branch(&git(), repo.path(), "main", &tip, &c())
            .await
            .is_err()
    );
    assert_eq!(current_branch(&repo).await, "main");
}

#[tokio::test]
async fn switches_carrying_changes_that_do_not_conflict() {
    let repo = repo_with_feature().await;
    repo.write_file("main.txt", "edited on main\n");
    let outcome = checkout_branch(&git(), repo.path(), "feature", &c())
        .await
        .expect("checkout");
    // main.txt does not exist on feature: git refuses to drop the edit.
    assert!(matches!(outcome, CheckoutOutcome::Blocked { .. }));

    repo.run(&["switch", "--quiet", "main"]).await;
    repo.write_file("new.txt", "untracked, not on feature\n");
    std::fs::write(repo.path().join("main.txt"), "main\n").expect("revert edit");
    let outcome = checkout_branch(&git(), repo.path(), "feature", &c())
        .await
        .expect("checkout");
    assert_eq!(outcome, CheckoutOutcome::Switched);
    assert_eq!(current_branch(&repo).await, "feature");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("new.txt")).expect("read"),
        "untracked, not on feature\n"
    );
}

#[tokio::test]
async fn never_forces_a_switch_over_uncommitted_changes() {
    let repo = repo_with_feature().await;
    repo.write_file("shared.txt", "staged on main\n");
    repo.run(&["add", "shared.txt"]).await;
    repo.write_file("shared.txt", "staged on main\nand unstaged\n");
    repo.write_file("other.txt", "unrelated\n");
    let index = repo.run(&["write-tree"]).await;

    let outcome = checkout_branch(&git(), repo.path(), "feature", &c())
        .await
        .expect("checkout");
    assert_eq!(
        outcome,
        CheckoutOutcome::Blocked {
            files: vec![BlockingFile {
                path: "shared.txt".into(),
                untracked: false
            }],
            changed: 2,
        }
    );
    assert_eq!(current_branch(&repo).await, "main");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("shared.txt")).expect("read"),
        "staged on main\nand unstaged\n"
    );
    assert_eq!(repo.run(&["write-tree"]).await, index, "index untouched");
}

#[tokio::test]
async fn untracked_files_in_the_way_block_too() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "base").await;
    repo.create_branch("other").await;
    repo.checkout("other").await;
    repo.commit_file("dir/b.txt", "from other\n", "add b").await;
    repo.checkout("main").await;
    repo.write_file("dir/b.txt", "mine, untracked\n");

    let outcome = checkout_branch(&git(), repo.path(), "other", &c())
        .await
        .expect("checkout");
    assert_eq!(
        outcome,
        CheckoutOutcome::Blocked {
            files: vec![BlockingFile {
                path: "dir/".into(),
                untracked: true
            }],
            changed: 1,
        }
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("dir/b.txt")).expect("read"),
        "mine, untracked\n"
    );

    let outcome = stash_and_checkout(&git(), repo.path(), "other", true, &c())
        .await
        .expect("stash and switch");
    assert_eq!(outcome, CheckoutOutcome::Switched);
    assert_eq!(
        repo.run(&["stash", "list", "--format=%gs"]).await,
        "On main: GitCRAFT: antes de cambiar a other"
    );
    // Untracked files live in the stash's third parent.
    assert_eq!(
        repo.run(&["show", "stash@{0}^3:dir/b.txt"]).await,
        "mine, untracked"
    );
}

#[tokio::test]
async fn stash_and_switch_keeps_the_changes_byte_for_byte() {
    let repo = repo_with_feature().await;
    repo.write_file("shared.txt", "staged\r\n");
    repo.run(&["add", "shared.txt"]).await;
    repo.write_file("shared.txt", "staged\r\nunstaged, no newline");
    let outcome = stash_and_checkout(&git(), repo.path(), "feature", false, &c())
        .await
        .expect("stash and switch");
    assert_eq!(outcome, CheckoutOutcome::Switched);
    assert_eq!(current_branch(&repo).await, "feature");

    let worktree = repo
        .read()
        .args(["show", "stash@{0}:shared.txt"])
        .output(&c())
        .await
        .expect("show");
    assert_eq!(worktree.stdout, b"staged\r\nunstaged, no newline");
    let index = repo
        .read()
        .args(["show", "stash@{0}^2:shared.txt"])
        .output(&c())
        .await
        .expect("show index");
    assert_eq!(index.stdout, b"staged\r\n");
}
