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
    assert_eq!(err.kind(), ErrorKind::RemoteUnreachable);
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
