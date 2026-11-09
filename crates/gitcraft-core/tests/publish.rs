//! Publishing on GitHub with a simulated gh (the `fake-gh` test binary, no
//! network) and a local bare repository as the "GitHub" remote.

use std::path::{Path, PathBuf};

use gitcraft_core::gh::Gh;
use gitcraft_core::ops::publish::{self, PublishRequest};
use gitcraft_core::ops::remote::{self, PushOutcome};
use gitcraft_core::testing::TestRepo;
use gitcraft_core::testing::fake_gh::{FakeGh, Reply, Scenario};
use gitcraft_core::{CancellationToken, ErrorKind, Git};

fn c() -> CancellationToken {
    CancellationToken::new()
}

fn git() -> Git {
    gitcraft_core::testing::isolated_git()
}

const SIGNED_IN: &str = r#"{"hosts":{"github.com":[{"state":"success","active":true,"login":"ana","gitProtocol":"PROTOCOL"}]}}"#;

const FAKE_GH: &str = env!("CARGO_BIN_EXE_fake-gh");

/// A `gh` that records its calls. `exists`: whether the name is taken;
/// `clone_url`: what the API says about the created repository.
struct Fake {
    sim: FakeGh,
    gh: Gh,
}

impl Fake {
    fn new(signed_in: bool, protocol: &str, exists: bool, clone_url: &str) -> Self {
        let status = if signed_in {
            SIGNED_IN.replace("PROTOCOL", protocol)
        } else {
            r#"{"hosts":{}}"#.to_owned()
        };
        let view = if exists {
            Reply::ok().stdout("HTTP/2.0 200 OK\n\n{}\n")
        } else {
            Reply::ok().stdout("HTTP/2.0 404 Not Found\n\n{}\n").exit(1)
        };
        let created = serde_json::json!({
            "full_name": "ana/x",
            "html_url": "https://github.com/ana/x",
            "clone_url": clone_url,
            "ssh_url": "git@github.com:ana/x.git",
        });
        Self::with(
            Scenario::always(Reply::ok())
                .when(&["auth", "status"], Reply::ok().out_line(&status))
                .when(
                    &["api", "--include", "-X"],
                    Reply::ok().stdout(&format!("HTTP/2.0 201 Created\n\n{created}\n")),
                )
                .when(&["api", "--include"], view),
        )
    }

    fn with(scenario: Scenario) -> Self {
        let sim = FakeGh::new(FAKE_GH, &scenario);
        let gh = Gh::at(sim.path());
        Self { sim, gh }
    }

    fn calls(&self) -> Vec<String> {
        self.sim.calls()
    }
}

/// A bare repository standing for the one GitHub creates: `<dir>/x.git`.
async fn github_side() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let bare = dir.path().join("x.git");
    let out = tokio::process::Command::new("git")
        .args(["init", "--bare", "-q"])
        .arg(&bare)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .await
        .expect("git init --bare");
    assert!(out.status.success());
    (dir, file_url(&bare))
}

/// `file:///tmp/x.git` on Unix, `file:///C:/Users/…/x.git` on Windows.
fn file_url(path: &Path) -> String {
    let path = path.display().to_string().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

fn request(name: &str) -> PublishRequest {
    PublishRequest {
        name: name.into(),
        description: Some("-probando, con comas".into()),
        private: true,
    }
}

#[tokio::test]
async fn previews_what_would_be_uploaded_with_warnings() {
    let repo = TestRepo::new().await;
    repo.commit_file("src/main.rs", "fn main() {}\n", "first")
        .await;
    repo.commit_file("node_modules/left-pad/index.js", "x\n", "deps")
        .await;
    repo.commit_file("app/target/out.bin", &"0".repeat(3000), "build")
        .await;
    let preview = publish::preview(&git(), repo.path(), 2000, &c())
        .await
        .expect("preview");
    assert_eq!(preview.branch, "main");
    assert_eq!(preview.commits, 3);
    assert!(preview.size_bytes > 0);
    assert!(!preview.has_gitignore);
    assert_eq!(preview.build_dirs, ["app/target", "node_modules"]);
    assert_eq!(preview.large_files.len(), 1);
    assert_eq!(preview.large_files[0].path, "app/target/out.bin");
    assert_eq!(preview.large_files[0].size, 3000);

    repo.commit_file(".gitignore", "target/\n", "ignore").await;
    let preview = publish::preview(&git(), repo.path(), 2000, &c())
        .await
        .expect("preview");
    assert!(preview.has_gitignore);
    // A large file stays in the history even if it is no longer in the tree.
    repo.run(&["rm", "-q", "app/target/out.bin"]).await;
    repo.commit_with(&["-m", "drop"]).await;
    let preview = publish::preview(&git(), repo.path(), 2000, &c())
        .await
        .expect("preview");
    assert_eq!(preview.large_files.len(), 1);
}

#[tokio::test]
async fn says_how_many_large_files_were_left_out() {
    let repo = TestRepo::new().await;
    for i in 0..23 {
        repo.write_file(&format!("f{i}.bin"), &"x".repeat(100 + i));
    }
    repo.commit_all("big files").await;
    let preview = publish::preview(&git(), repo.path(), 50, &c())
        .await
        .expect("preview");
    assert_eq!(preview.large_files.len(), 20);
    assert_eq!(preview.more_large_files, 3);
    assert_eq!(preview.more_build_dirs, 0);
    // The largest ones are the ones listed.
    assert_eq!(preview.large_files[0].path, "f22.bin");
}

#[tokio::test]
async fn refuses_without_commits_or_on_a_detached_head() {
    let repo = TestRepo::new().await;
    let err = publish::preview(&git(), repo.path(), 1, &c())
        .await
        .expect_err("no commits");
    assert_eq!(err.kind(), ErrorKind::NoCommits);

    let first = repo.commit_file("a.txt", "a\n", "a").await;
    repo.run(&["checkout", "-q", "--detach", &first]).await;
    let err = publish::preview(&git(), repo.path(), 1, &c())
        .await
        .expect_err("detached");
    assert_eq!(err.kind(), ErrorKind::DetachedHead);
}

#[tokio::test]
async fn creates_the_private_repository_adds_origin_and_pushes_with_upstream() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "a").await;
    let (_github, url) = github_side().await;
    let fake = Fake::new(true, "https", false, &url);

    let published = publish::publish(&git(), &fake.gh, repo.path(), &request("x"), &c())
        .await
        .expect("publish");
    assert_eq!(published.full_name, "ana/x");
    assert_eq!(published.remote_url, url);
    assert_eq!(
        fake.calls(),
        [
            "auth status --hostname github.com --json hosts",
            "api --include repos/ana/x",
            "api --include -X POST user/repos -f name=x -F private=true -f description=-probando, con comas",
        ]
    );
    assert_eq!(repo.run(&["remote", "get-url", "origin"]).await.trim(), url);

    // The normal push publishes the branch: never forced, with upstream.
    let outcome = remote::push(&git(), repo.path(), Some("origin"), None, &c(), |_| {})
        .await
        .expect("push");
    assert!(matches!(outcome, PushOutcome::Pushed { .. }), "{outcome:?}");
    assert_eq!(
        repo.run(&["rev-parse", "--abbrev-ref", "main@{upstream}"])
            .await
            .trim(),
        "origin/main"
    );
}

#[tokio::test]
async fn an_existing_name_changes_nothing() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "a").await;
    let fake = Fake::new(true, "https", true, "unused");
    let err = publish::publish(&git(), &fake.gh, repo.path(), &request("x"), &c())
        .await
        .expect_err("exists");
    assert_eq!(err.kind(), ErrorKind::GhRepoExists);
    assert!(!fake.calls().iter().any(|c| c.contains("-X POST")));
    assert!(
        repo.run(&["remote"]).await.trim().is_empty(),
        "no origin added"
    );
}

#[tokio::test]
async fn needs_a_session_a_valid_name_and_no_remote() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "a").await;

    let fake = Fake::new(false, "https", false, "unused");
    let err = publish::publish(&git(), &fake.gh, repo.path(), &request("x"), &c())
        .await
        .expect_err("no session");
    assert_eq!(err.kind(), ErrorKind::GhNotLoggedIn);

    let fake = Fake::new(true, "https", false, "unused");
    let err = publish::publish(&git(), &fake.gh, repo.path(), &request("-x"), &c())
        .await
        .expect_err("bad name");
    assert_eq!(err.kind(), ErrorKind::InvalidRepoName);
    assert!(!fake.calls().iter().any(|c| c.starts_with("api ")));

    repo.run(&["remote", "add", "other", "https://example.com/r.git"])
        .await;
    let fake = Fake::new(true, "https", false, "unused");
    let err = publish::publish(&git(), &fake.gh, repo.path(), &request("x"), &c())
        .await
        .expect_err("has remote");
    assert_eq!(err.kind(), ErrorKind::HasRemote);
    assert!(fake.calls().is_empty(), "gh is not even asked");
}

#[tokio::test]
async fn without_connection_nothing_is_created_and_it_says_so() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "a").await;
    let fake = Fake::with(
        Scenario::always(Reply::ok().err_line("dial tcp: no such host").exit(1)).when(
            &["auth", "status"],
            Reply::ok().out_line(
                r#"{"hosts":{"github.com":[{"state":"error","active":true,"login":"ana"}]}}"#,
            ),
        ),
    );
    let err = publish::publish(&git(), &fake.gh, repo.path(), &request("x"), &c())
        .await
        .expect_err("no connection");
    assert_eq!(err.kind(), ErrorKind::GhUnreachable);
    let calls = fake.calls().join("\n");
    assert!(!calls.contains("POST"), "{calls}");
    assert!(repo.run(&["remote"]).await.trim().is_empty());
}

#[tokio::test]
async fn a_push_that_fails_after_creating_leaves_origin_to_retry() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "a").await;
    let gone = PathBuf::from("/nonexistent/gitcraft-test/x");
    let fake = Fake::new(true, "https", false, &file_url(&gone));
    publish::publish(&git(), &fake.gh, repo.path(), &request("x"), &c())
        .await
        .expect("created");
    let err = remote::push(&git(), repo.path(), Some("origin"), None, &c(), |_| {})
        .await
        .expect_err("push fails");
    assert_ne!(err.kind(), ErrorKind::Cancelled);
    assert_eq!(
        repo.run(&["remote"]).await.trim(),
        "origin",
        "kept to retry"
    );
}

#[tokio::test]
async fn uses_the_ssh_url_when_the_account_uses_ssh() {
    let repo = TestRepo::new().await;
    repo.commit_file("a.txt", "a\n", "a").await;
    let fake = Fake::new(true, "ssh", false, "https://github.com/ana/x.git");
    let published = publish::publish(&git(), &fake.gh, repo.path(), &request("x"), &c())
        .await
        .expect("publish");
    assert_eq!(published.remote_url, "git@github.com:ana/x.git");
    assert_eq!(published.web_url, "https://github.com/ana/x");
}
