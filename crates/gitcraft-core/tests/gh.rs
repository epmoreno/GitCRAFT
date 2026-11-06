//! gh, simulated: a script stands in for it, so no test needs the network
//! or touches the real gh session.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gitcraft_core::gh::{Gh, GhAccount, LoginEvent};
use gitcraft_core::{CancellationToken, ErrorKind, GitError};

struct FakeGh {
    dir: tempfile::TempDir,
    gh: Gh,
}

impl FakeGh {
    /// A `gh` that records its arguments and runs `body` (sh) for them.
    fn new(body: &str) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let exe = dir.path().join("gh");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"{log}\"\n{body}\n",
            log = dir.path().join("calls.log").display()
        );
        std::fs::write(&exe, script).expect("write fake gh");
        make_executable(&exe);
        let gh = Gh::at(&exe);
        Self { dir, gh }
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

const LOGGED_IN: &str = r#"echo '{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"ana","tokenSource":"keyring"}]}}'"#;

#[tokio::test]
async fn a_missing_gh_is_reported_as_not_installed() {
    let gh = Gh::at(PathBuf::from("/nonexistent/gh"));
    let err = gh.accounts(&c()).await.expect_err("missing");
    assert_eq!(err.kind(), ErrorKind::GhNotFound);
}

#[tokio::test]
async fn reads_the_session_and_its_user() {
    let fake = FakeGh::new(LOGGED_IN);
    assert_eq!(
        fake.gh.accounts(&c()).await.expect("status"),
        [GhAccount {
            login: "ana".into(),
            active: true,
            valid: true,
            git_protocol: "https".into(),
        }]
    );
    assert_eq!(
        fake.calls(),
        ["auth status --hostname github.com --json hosts"]
    );
}

#[tokio::test]
async fn no_session_and_an_expired_token() {
    let fake =
        FakeGh::new(r#"echo '{"hosts":{}}'; echo 'You are not logged into any GitHub hosts.' >&2"#);
    assert!(fake.gh.accounts(&c()).await.expect("status").is_empty());

    let fake = FakeGh::new(
        r#"echo '{"hosts":{"github.com":[{"state":"error","active":true,"login":"ana"}]}}'"#,
    );
    let accounts = fake.gh.accounts(&c()).await.expect("status");
    assert!(!accounts[0].valid, "expired or revoked token");
}

#[tokio::test]
async fn signs_in_reporting_the_one_time_code() {
    let fake = FakeGh::new(
        "echo '! One-time code (WXYZ-1234) copied to clipboard' >&2\n\
         echo 'Open this URL to continue in your web browser: https://github.com/login/device' >&2\n\
         echo '✓ Logged in as ana' >&2",
    );
    let mut events = Vec::new();
    fake.gh
        .login(|e| events.push(e), &c())
        .await
        .expect("login");
    assert_eq!(
        events[0],
        LoginEvent::Code {
            code: "WXYZ-1234".into(),
            url: "https://github.com/login/device".into()
        }
    );
    assert_eq!(
        fake.calls(),
        ["auth login --hostname github.com --web --git-protocol https --skip-ssh-key"]
    );
}

#[tokio::test]
async fn cancelling_the_sign_in_stops_gh() {
    let fake = FakeGh::new("echo '! One-time code (WXYZ-1234) copied' >&2\nsleep 30");
    let cancel = c();
    let stop = cancel.clone();
    let started = std::time::Instant::now();
    let err = fake
        .gh
        .login(
            move |e| {
                if matches!(e, LoginEvent::Code { .. }) {
                    stop.cancel();
                }
            },
            &cancel,
        )
        .await
        .expect_err("cancelled");
    assert!(matches!(err, GitError::Cancelled));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn a_failed_sign_in_keeps_what_gh_said() {
    let fake = FakeGh::new("echo 'error connecting to api.github.com' >&2; exit 1");
    let err = fake.gh.login(|_| {}, &c()).await.expect_err("fails");
    assert_eq!(err.kind(), ErrorKind::GhFailed);
    assert!(err.details().expect("details").contains("error connecting"));
}

#[tokio::test]
async fn signs_out_and_sets_up_git_for_one_user_on_github_com() {
    let fake = FakeGh::new("true");
    fake.gh.logout("-ana", &c()).await.expect("logout");
    fake.gh.setup_git(&c()).await.expect("setup-git");
    assert_eq!(
        fake.calls(),
        [
            "auth logout --hostname github.com --user -ana",
            "auth setup-git --hostname github.com"
        ]
    );
}

#[tokio::test]
async fn never_asks_gh_for_a_token() {
    let fake = FakeGh::new(LOGGED_IN);
    fake.gh.version(&c()).await.expect("version");
    fake.gh.accounts(&c()).await.expect("status");
    fake.gh.logout("ana", &c()).await.expect("logout");
    for call in fake.calls() {
        assert!(!call.contains("token"), "{call}");
    }
}

/// What `gh api --include` writes for an HTTP answer.
fn answer(status: &str, body: &str, exit: i32) -> String {
    format!(
        "printf 'HTTP/2.0 {status}\\nContent-Type: application/json\\n\\n%s\\n' '{body}'; exit {exit}"
    )
}

#[tokio::test]
async fn the_http_status_says_whether_a_repository_exists() {
    let fake = FakeGh::new(&answer("200 OK", "{}", 0));
    assert!(fake.gh.repo_exists("ana", "x", &c()).await.expect("200"));
    assert_eq!(fake.calls(), ["api --include repos/ana/x"]);

    // Nothing depends on gh's words: only the status counts.
    let fake = FakeGh::new(&format!(
        "echo 'mensaje cualquiera' >&2; {}",
        answer("404 Not Found", "{}", 1)
    ));
    assert!(!fake.gh.repo_exists("ana", "x", &c()).await.expect("404"));

    let fake = FakeGh::new(&format!(
        "echo 'Could not resolve to a Repository' >&2; {}",
        answer("500 Server Error", "{}", 1)
    ));
    let err = fake
        .gh
        .repo_exists("ana", "x", &c())
        .await
        .expect_err("500");
    assert_eq!(err.kind(), ErrorKind::GhFailed);
}

#[tokio::test]
async fn classifies_failures_by_exit_code_and_status_not_by_wording() {
    // 401: the session is not valid, whatever gh says.
    let fake = FakeGh::new(&answer("401 Unauthorized", "{}", 1));
    let err = fake
        .gh
        .repo_exists("ana", "x", &c())
        .await
        .expect_err("401");
    assert_eq!(err.kind(), ErrorKind::GhNotLoggedIn);

    // Documented exit code 4: authentication needed.
    let fake = FakeGh::new("echo 'lo que sea' >&2; exit 4");
    let err = fake.gh.repo_exists("ana", "x", &c()).await.expect_err("4");
    assert_eq!(err.kind(), ErrorKind::GhNotLoggedIn);
    let err = fake
        .gh
        .setup_git(&c())
        .await
        .expect_err("4 outside the API");
    assert_eq!(err.kind(), ErrorKind::GhNotLoggedIn);

    // No HTTP answer at all: GitHub could not be reached. The output stays.
    let fake = FakeGh::new("echo 'dial tcp: sin red' >&2; exit 1");
    let err = fake
        .gh
        .repo_exists("ana", "x", &c())
        .await
        .expect_err("no answer");
    assert_eq!(err.kind(), ErrorKind::RemoteUnreachable);
    assert!(err.details().expect("details").contains("sin red"));

    // Words that used to mean something no longer do.
    let fake = FakeGh::new("echo 'not logged into github.com. gh auth login' >&2; exit 1");
    let err = fake.gh.setup_git(&c()).await.expect_err("exit 1");
    assert_eq!(err.kind(), ErrorKind::GhFailed);
}

#[tokio::test]
async fn creates_in_the_personal_account_from_the_json_answer() {
    let body = r#"{"full_name":"ana/x","html_url":"https://github.com/ana/x","clone_url":"https://github.com/ana/x.git","ssh_url":"git@github.com:ana/x.git"}"#;
    let fake = FakeGh::new(&answer("201 Created", body, 0));
    let created = fake
        .gh
        .create_repo("x", Some("  -con guion "), true, &c())
        .await
        .expect("created");
    assert_eq!(created.full_name, "ana/x");
    assert_eq!(created.ssh_url, "git@github.com:ana/x.git");
    assert_eq!(
        fake.calls(),
        ["api --include -X POST user/repos -f name=x -F private=true -f description=-con guion"]
    );
}

#[tokio::test]
async fn a_name_taken_meanwhile_is_reported_as_existing() {
    let taken = r#"{"message":"Repository creation failed.","errors":[{"resource":"Repository","code":"custom","field":"name"}]}"#;
    let fake = FakeGh::new(&answer("422 Unprocessable Entity", taken, 1));
    let err = fake
        .gh
        .create_repo("x", None, true, &c())
        .await
        .expect_err("422");
    assert_eq!(err.kind(), ErrorKind::GhRepoExists);

    // Another validation error stays a plain failure, with GitHub's answer.
    let other = r#"{"message":"Validation Failed","errors":[{"field":"description"}]}"#;
    let fake = FakeGh::new(&answer("422 Unprocessable Entity", other, 1));
    let err = fake
        .gh
        .create_repo("x", None, false, &c())
        .await
        .expect_err("422");
    assert_eq!(err.kind(), ErrorKind::GhFailed);
    assert!(
        err.details()
            .expect("details")
            .contains("Validation Failed")
    );
}

/// The login's one-time code is the only thing read from gh's human output.
/// (`error.rs` also matches git's messages, which `LC_ALL=C` keeps stable.)
#[test]
fn decisions_never_look_at_what_gh_says() {
    for source in [
        include_str!("../src/gh.rs"),
        include_str!("../src/ops/publish.rs"),
    ] {
        for forbidden in ["stderr.contains", "said.contains", "stdout.contains"] {
            assert!(!source.contains(forbidden), "{forbidden}");
        }
    }
    let errors = include_str!("../src/error.rs");
    for phrase in ["not logged into", "gh auth login", "error connecting to"] {
        assert!(!errors.contains(phrase), "{phrase}");
    }
}
