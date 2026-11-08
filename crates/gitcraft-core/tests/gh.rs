//! gh, simulated: the `fake-gh` test binary stands in for it, so no test
//! needs the network or touches the real gh session.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gitcraft_core::gh::{Gh, GhAccount, LoginEvent};
use gitcraft_core::testing::fake_gh::{FakeGh, Reply, Scenario};
use gitcraft_core::{
    Access, CancellationToken, CommandEvent, CommandLog, CommandOutcome, ErrorKind, GitError,
};

const FAKE_GH: &str = env!("CARGO_BIN_EXE_fake-gh");

struct Fake {
    sim: FakeGh,
    gh: Gh,
}

impl Fake {
    /// A `gh` that records its arguments and answers as `scenario` says.
    fn new(scenario: Scenario) -> Self {
        let sim = FakeGh::new(FAKE_GH, &scenario);
        let gh = Gh::at(sim.path());
        Self { sim, gh }
    }

    /// A `gh` that gives `reply` to every call.
    fn always(reply: Reply) -> Self {
        Self::new(Scenario::always(reply))
    }

    fn calls(&self) -> Vec<String> {
        self.sim.calls()
    }
}

fn c() -> CancellationToken {
    CancellationToken::new()
}

/// `gh auth status --json hosts` with a working session for "ana".
fn logged_in() -> Reply {
    Reply::ok().out_line(
        r#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"ana","tokenSource":"keyring"}]}}"#,
    )
}

/// Waits far longer than any test: only a time limit or a cancel ends it.
fn forever() -> Reply {
    Reply::ok().sleep(Duration::from_secs(30))
}

#[tokio::test]
async fn a_missing_gh_is_reported_as_not_installed() {
    let gh = Gh::at(PathBuf::from("/nonexistent/gh"));
    let err = gh.accounts(&c()).await.expect_err("missing");
    assert_eq!(err.kind(), ErrorKind::GhNotFound);
}

#[tokio::test]
async fn reads_the_session_and_its_user() {
    let fake = Fake::always(logged_in());
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
    let fake = Fake::always(
        Reply::ok()
            .out_line(r#"{"hosts":{}}"#)
            .err_line("You are not logged into any GitHub hosts."),
    );
    assert!(fake.gh.accounts(&c()).await.expect("status").is_empty());

    let fake = Fake::always(token_not_working());
    let accounts = fake.gh.accounts(&c()).await.expect("status");
    assert!(!accounts[0].valid, "expired or revoked token");
}

#[tokio::test]
async fn signs_in_reporting_the_one_time_code() {
    let fake = Fake::always(
        Reply::ok()
            .err_line("! One-time code (WXYZ-1234) copied to clipboard")
            .err_line(
                "Open this URL to continue in your web browser: https://github.com/login/device",
            )
            .err_line("✓ Logged in as ana"),
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
    let fake = Fake::always(forever().err_line("! One-time code (WXYZ-1234) copied"));
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
    let fake = Fake::always(
        Reply::ok()
            .err_line("error connecting to api.github.com")
            .exit(1),
    );
    let err = fake.gh.login(|_| {}, &c()).await.expect_err("fails");
    assert_eq!(err.kind(), ErrorKind::GhFailed);
    assert!(err.details().expect("details").contains("error connecting"));
}

#[tokio::test]
async fn signs_out_and_sets_up_git_for_one_user_on_github_com() {
    let fake = Fake::always(Reply::ok());
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
    let fake = Fake::always(logged_in());
    fake.gh.version(&c()).await.expect("version");
    fake.gh.accounts(&c()).await.expect("status");
    fake.gh.logout("ana", &c()).await.expect("logout");
    for call in fake.calls() {
        assert!(!call.contains("token"), "{call}");
    }
}

#[tokio::test]
async fn the_http_status_says_whether_a_repository_exists() {
    let fake = Fake::always(Reply::http("200 OK", "{}", 0));
    assert!(fake.gh.repo_exists("ana", "x", &c()).await.expect("200"));
    assert_eq!(fake.calls(), ["api --include repos/ana/x"]);

    // Nothing depends on gh's words: only the status counts.
    let fake = Fake::always(Reply::http("404 Not Found", "{}", 1).err_line("mensaje cualquiera"));
    assert!(!fake.gh.repo_exists("ana", "x", &c()).await.expect("404"));

    let fake = Fake::always(
        Reply::http("500 Server Error", "{}", 1).err_line("Could not resolve to a Repository"),
    );
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
    let fake = Fake::always(Reply::http("401 Unauthorized", "{}", 1));
    let err = fake
        .gh
        .repo_exists("ana", "x", &c())
        .await
        .expect_err("401");
    assert_eq!(err.kind(), ErrorKind::GhNotLoggedIn);

    // Documented exit code 4: authentication needed.
    let fake = Fake::always(Reply::ok().err_line("lo que sea").exit(4));
    let err = fake.gh.repo_exists("ana", "x", &c()).await.expect_err("4");
    assert_eq!(err.kind(), ErrorKind::GhNotLoggedIn);
    let err = fake
        .gh
        .setup_git(&c())
        .await
        .expect_err("4 outside the API");
    assert_eq!(err.kind(), ErrorKind::GhNotLoggedIn);

    // No HTTP answer at all: GitHub could not be reached. The output stays.
    let fake = Fake::always(Reply::ok().err_line("dial tcp: sin red").exit(1));
    let err = fake
        .gh
        .repo_exists("ana", "x", &c())
        .await
        .expect_err("no answer");
    assert_eq!(err.kind(), ErrorKind::GhUnreachable);
    assert!(err.details().expect("details").contains("sin red"));

    // Words that used to mean something no longer do.
    let fake = Fake::always(
        Reply::ok()
            .err_line("not logged into github.com. gh auth login")
            .exit(1),
    );
    let err = fake.gh.setup_git(&c()).await.expect_err("exit 1");
    assert_eq!(err.kind(), ErrorKind::GhFailed);
}

#[tokio::test]
async fn creates_in_the_personal_account_from_the_json_answer() {
    let body = r#"{"full_name":"ana/x","html_url":"https://github.com/ana/x","clone_url":"https://github.com/ana/x.git","ssh_url":"git@github.com:ana/x.git"}"#;
    let fake = Fake::always(Reply::http("201 Created", body, 0));
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
    let fake = Fake::always(Reply::http("422 Unprocessable Entity", taken, 1));
    let err = fake
        .gh
        .create_repo("x", None, true, &c())
        .await
        .expect_err("422");
    assert_eq!(err.kind(), ErrorKind::GhRepoExists);

    // Another validation error stays a plain failure, with GitHub's answer.
    let other = r#"{"message":"Validation Failed","errors":[{"field":"description"}]}"#;
    let fake = Fake::always(Reply::http("422 Unprocessable Entity", other, 1));
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

/// Keeps what the history would receive.
#[derive(Default)]
struct Recorder(Mutex<Vec<CommandEvent>>);

impl CommandLog for Recorder {
    fn record(&self, event: CommandEvent) {
        self.0.lock().expect("lock").push(event);
    }
}

fn logged(fake: &Fake) -> (Gh, Arc<Recorder>) {
    let recorder = Arc::new(Recorder::default());
    let gh = fake
        .gh
        .clone()
        .with_log(Arc::clone(&recorder) as Arc<dyn CommandLog>);
    (gh, recorder)
}

/// `(command, read or write, how it ended)` of each finished command.
fn summary(recorder: &Recorder) -> Vec<(String, Access, Option<CommandOutcome>)> {
    let events = recorder.0.lock().expect("lock");
    events
        .iter()
        .filter_map(|event| match event {
            CommandEvent::Started {
                id,
                command,
                access,
                ..
            } => {
                let outcome = events.iter().find_map(|e| match e {
                    CommandEvent::Finished {
                        id: end, outcome, ..
                    } if end == id => Some(*outcome),
                    _ => None,
                });
                Some((command.clone(), *access, outcome))
            }
            CommandEvent::Finished { .. } => None,
        })
        .collect()
}

#[tokio::test]
async fn records_gh_commands_in_the_history() {
    let fake = Fake::always(logged_in());
    let (gh, recorder) = logged(&fake);
    gh.accounts(&c()).await.expect("status");
    assert_eq!(
        summary(&recorder),
        [(
            "gh auth status --hostname github.com --json hosts".to_owned(),
            Access::Read,
            Some(CommandOutcome::Succeeded)
        )]
    );
}

#[tokio::test]
async fn a_404_that_answers_the_question_is_not_a_failure() {
    let fake = Fake::always(Reply::http("404 Not Found", "{}", 1));
    let (gh, recorder) = logged(&fake);
    assert!(!gh.repo_exists("ana", "x", &c()).await.expect("404"));
    assert_eq!(
        summary(&recorder)[0].2,
        Some(CommandOutcome::Succeeded),
        "the name is free: that is the answer"
    );

    let fake = Fake::always(Reply::http("500 Server Error", "{}", 1));
    let (gh, recorder) = logged(&fake);
    gh.repo_exists("ana", "x", &c()).await.expect_err("500");
    assert_eq!(
        summary(&recorder)[0].2,
        Some(CommandOutcome::Failed { code: Some(1) })
    );
}

#[tokio::test]
async fn records_failures_and_cancelled_commands() {
    let fake = Fake::always(Reply::ok().exit(3));
    let (gh, recorder) = logged(&fake);
    gh.setup_git(&c()).await.expect_err("fails");
    assert_eq!(
        summary(&recorder),
        [(
            "gh auth setup-git --hostname github.com".to_owned(),
            Access::Write,
            Some(CommandOutcome::Failed { code: Some(3) })
        )]
    );

    let fake = Fake::always(forever());
    let (gh, recorder) = logged(&fake);
    let cancel = c();
    cancel.cancel();
    gh.setup_git(&cancel).await.expect_err("cancelled");
    assert_eq!(summary(&recorder)[0].2, Some(CommandOutcome::Cancelled));
}

#[tokio::test]
async fn the_history_redacts_credentials_and_never_shows_the_one_time_code() {
    let body = r#"{"full_name":"a/x","html_url":"u","clone_url":"u","ssh_url":"u"}"#;
    let fake = Fake::always(Reply::http("201 Created", body, 0));
    let (gh, recorder) = logged(&fake);
    gh.create_repo(
        "x",
        Some("mira https://ana:secreto@example.com/r.git"),
        true,
        &c(),
    )
    .await
    .expect("created");
    let command = summary(&recorder)[0].0.clone();
    assert!(!command.contains("secreto"), "{command}");
    assert!(
        command.contains("https://***@example.com/r.git"),
        "{command}"
    );

    let fake =
        Fake::always(Reply::ok().err_line("! One-time code (WXYZ-1234) copied to clipboard"));
    let (gh, recorder) = logged(&fake);
    gh.login(|_| {}, &c()).await.expect("login");
    let command = summary(&recorder)[0].0.clone();
    assert!(command.starts_with("gh auth login"), "{command}");
    assert!(!command.contains("WXYZ"), "{command}");
}

#[tokio::test]
async fn gh_commands_about_a_repository_say_which_one() {
    let fake = Fake::always(logged_in());
    let dir = tempfile::tempdir().expect("tempdir");
    let folder = |recorder: &Recorder| {
        recorder
            .0
            .lock()
            .expect("lock")
            .iter()
            .find_map(|e| match e {
                CommandEvent::Started { cwd, .. } => Some(cwd.clone()),
                CommandEvent::Finished { .. } => None,
            })
    };
    let (gh, recorder) = logged(&fake);
    gh.accounts(&c()).await.expect("status");
    assert_eq!(
        folder(&recorder),
        Some(PathBuf::new()),
        "about no repository"
    );

    let (gh, recorder) = logged(&fake);
    gh.in_repo(dir.path()).accounts(&c()).await.expect("status");
    assert_eq!(folder(&recorder), Some(dir.path().to_owned()));
}

/// `gh` for the session checks: `status` answers `gh auth status` and
/// `user` the `gh api … user` call.
fn session_gh(status: Reply, user: Reply) -> Fake {
    Fake::new(
        Scenario::always(Reply::ok())
            .when(&["auth", "status"], status)
            .when(&["api", "--include"], user),
    )
}

/// `gh auth status` when the token does not work (revoked, or no network).
fn token_not_working() -> Reply {
    Reply::ok()
        .out_line(r#"{"hosts":{"github.com":[{"state":"error","active":true,"login":"ana"}]}}"#)
}
const SHORT: Duration = Duration::from_millis(300);

#[tokio::test]
async fn no_account_is_no_session_without_asking_github() {
    let fake = session_gh(Reply::ok().out_line(r#"{"hosts":{}}"#), Reply::ok().exit(9));
    let session = fake.gh.session(&c()).await.expect("session");
    assert!(session.accounts.is_empty());
    assert!(!session.unreachable);
    assert_eq!(fake.calls().len(), 1, "{:?}", fake.calls());
}

#[tokio::test]
async fn a_working_token_needs_no_further_check() {
    let fake = session_gh(logged_in(), Reply::ok().exit(9));
    let session = fake.gh.session(&c()).await.expect("session");
    assert!(session.accounts[0].valid);
    assert!(!session.unreachable);
    assert_eq!(fake.calls().len(), 1, "{:?}", fake.calls());
}

#[tokio::test]
async fn a_slow_status_means_it_could_not_be_checked() {
    let fake = session_gh(forever(), Reply::ok().exit(9));
    let gh = fake.gh.clone().with_check_timeout(SHORT);
    let started = std::time::Instant::now();
    let session = gh.session(&c()).await.expect("session");
    assert!(session.unreachable);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "does not wait for gh"
    );
}

#[tokio::test]
async fn a_status_that_times_out_is_recorded_as_such() {
    let fake = session_gh(forever(), Reply::ok().exit(9));
    let (gh, recorder) = logged(&fake);
    gh.with_check_timeout(SHORT)
        .session(&c())
        .await
        .expect("session");
    assert_eq!(summary(&recorder)[0].2, Some(CommandOutcome::TimedOut));
}

#[tokio::test]
async fn without_connection_a_token_that_does_not_work_is_not_called_invalid() {
    // What gh reports without a network: the token "does not work". Then the
    // check against GitHub gets no HTTP answer, or the state is "timeout".
    for status in [
        token_not_working(),
        Reply::ok().out_line(
            r#"{"hosts":{"github.com":[{"state":"timeout","active":true,"login":"ana"}]}}"#,
        ),
    ] {
        let label = status.stdout.clone();
        let fake = session_gh(
            status,
            Reply::ok().err_line("dial tcp: no such host").exit(1),
        );
        let session = fake.gh.session(&c()).await.expect("session");
        assert!(session.unreachable, "{label}");
        assert_eq!(session.accounts.len(), 1);
    }
}

#[tokio::test]
async fn the_second_check_also_has_a_time_limit() {
    let fake = session_gh(token_not_working(), forever());
    let gh = fake.gh.clone().with_check_timeout(SHORT);
    let started = std::time::Instant::now();
    let session = gh.session(&c()).await.expect("session");
    assert!(session.unreachable);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn only_github_can_say_a_token_is_invalid() {
    let fake = session_gh(
        token_not_working(),
        Reply::http("401 Unauthorized", "{}", 1),
    );
    let session = fake.gh.session(&c()).await.expect("session");
    assert!(!session.unreachable);
    assert!(!session.accounts[0].valid, "really invalid");
    assert!(fake.calls().contains(&"api --include user".to_owned()));

    // Authentication needed, by gh's documented exit code.
    let fake = session_gh(token_not_working(), Reply::ok().exit(4));
    let session = fake.gh.session(&c()).await.expect("session");
    assert!(!session.unreachable);

    // The token works after all.
    let fake = session_gh(token_not_working(), Reply::http("200 OK", "{}", 0));
    let session = fake.gh.session(&c()).await.expect("session");
    assert!(!session.unreachable);
    assert!(session.accounts[0].valid);

    // GitHub answered, but not about the token.
    let fake = session_gh(token_not_working(), Reply::http("503 Unavailable", "{}", 1));
    let session = fake.gh.session(&c()).await.expect("session");
    assert!(session.unreachable);
}

#[tokio::test]
async fn checking_a_name_also_has_a_time_limit() {
    let fake = Fake::always(forever());
    let gh = fake.gh.clone().with_check_timeout(SHORT);
    let err = gh.repo_exists("ana", "x", &c()).await.expect_err("slow");
    assert_eq!(err.kind(), ErrorKind::GhUnreachable);
}
