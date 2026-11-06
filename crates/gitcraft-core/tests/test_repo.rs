//! Tests for the `TestRepo` fixture itself: other tests rely on its guarantees.

use gitcraft_core::testing::{AUTHOR_EMAIL, AUTHOR_NAME, EPOCH, TestRepo};

#[tokio::test]
async fn new_repo_is_empty_on_main() {
    let repo = TestRepo::new().await;
    assert!(repo.path().join(".git").is_dir());
    assert_eq!(repo.run(&["symbolic-ref", "HEAD"]).await, "refs/heads/main");
    assert_eq!(repo.run(&["rev-list", "--all", "--count"]).await, "0");
}

#[tokio::test]
async fn uses_fixed_identity_and_fake_clock() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "first").await;
    repo.commit_file("a.txt", "b\n", "second").await;

    let log = repo.run(&["log", "--format=%an <%ae> %at %ct %s"]).await;
    let expected = format!(
        "{AUTHOR_NAME} <{AUTHOR_EMAIL}> {t2} {t2} second\n{AUTHOR_NAME} <{AUTHOR_EMAIL}> {t1} {t1} first",
        t1 = EPOCH + 60,
        t2 = EPOCH + 120,
    );
    assert_eq!(log, expected);
}

#[tokio::test]
async fn ignores_the_developers_git_config() {
    let repo = TestRepo::new().await;
    // With global and system config disabled only the repo's own config and
    // the runner's `-c` overrides remain.
    let origins = repo.run(&["config", "--list", "--show-origin"]).await;
    for line in origins.lines() {
        assert!(
            line.starts_with("file:.git/config") || line.starts_with("command line:"),
            "{line}"
        );
    }
}

#[tokio::test]
async fn sample_history_has_the_documented_shape() {
    let (repo, ids) = TestRepo::sample_history().await;

    let branches = repo
        .run(&[
            "for-each-ref",
            "--format=%(refname:short) %(objectname)",
            "refs/heads",
        ])
        .await;
    assert_eq!(
        branches,
        format!(
            "feature {}\nmain {}\ntopic {}",
            ids["D"], ids["E"], ids["F"]
        )
    );

    // M merges B (first parent) with D.
    let merge_parents = repo.run(&["rev-list", "--parents", "-n1", &ids["M"]]).await;
    assert_eq!(
        merge_parents,
        format!("{} {} {}", ids["M"], ids["B"], ids["D"])
    );

    // Both tag kinds point where expected.
    assert_eq!(repo.run(&["cat-file", "-t", "v1.0"]).await, "tag");
    assert_eq!(repo.rev_parse("v1.0^{commit}").await, ids["B"]);
    assert_eq!(repo.run(&["cat-file", "-t", "v2.0"]).await, "commit");
    assert_eq!(repo.rev_parse("v2.0").await, ids["E"]);

    // topic is not merged into main.
    let unmerged = repo
        .run(&["branch", "--no-merged", "main", "--format=%(refname:short)"])
        .await;
    assert_eq!(unmerged, "topic");

    assert_eq!(repo.run(&["rev-list", "--all", "--count"]).await, "7");
    assert_eq!(repo.run(&["status", "--porcelain"]).await, "");
}

#[tokio::test]
async fn sample_history_is_deterministic() {
    let (_first, a) = TestRepo::sample_history().await;
    let (_second, b) = TestRepo::sample_history().await;
    assert_eq!(a, b);
}

/// Every test goes through `testing::isolated_git()`.
#[test]
fn no_test_uses_the_real_git_configuration() {
    let forbidden = concat!("Git::", "discover(");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<_> = std::fs::read_dir(root.join("tests"))
        .expect("tests dir")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "rs"))
        .collect();
    assert!(files.len() > 10, "{files:?}");
    files.push(root.join("../../src-tauri/src/state.rs"));
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read");
        assert!(!text.contains(forbidden), "{}", file.display());
    }
}

/// A global config that would change how git behaves is not even read.
#[tokio::test]
async fn a_hostile_global_config_has_no_effect() {
    let home = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        home.path().join(".gitconfig"),
        "[pull]\n\trebase = true\n[commit]\n\tgpgsign = true\n[init]\n\tdefaultBranch = raro\n\
         [user]\n\tname = Intruso\n",
    )
    .expect("write");
    let git = gitcraft_core::testing::isolated_git()
        .with_env("HOME", home.path())
        .with_env("XDG_CONFIG_HOME", home.path());
    let cancel = gitcraft_core::CancellationToken::new();
    for key in [
        "pull.rebase",
        "commit.gpgsign",
        "init.defaultBranch",
        "user.name",
    ] {
        let out = git
            .read(home.path())
            .args(["config", "--get", key])
            .accept_exit_code(1)
            .output(&cancel)
            .await
            .expect("git config");
        assert_eq!(out.exit_code, Some(1), "{key} must not be read");
    }
}
