//! Integration tests for the git runner against the real `git` binary.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gitcraft_core::{
    Access, CancellationToken, CommandEvent, CommandLog, CommandOutcome, Git, GitCommand, GitError,
};

fn git() -> Git {
    gitcraft_core::testing::isolated_git()
}

/// Isolates a command from the developer's global and system git config.
fn isolated(cmd: GitCommand) -> GitCommand {
    cmd.env("GIT_CONFIG_GLOBAL", gitcraft_core::testing::NO_CONFIG)
        .env("GIT_CONFIG_NOSYSTEM", "1")
}

fn read_in(dir: &Path) -> GitCommand {
    isolated(git().read(dir))
}

fn stdout_of(output: &gitcraft_core::GitOutput) -> String {
    output.stdout_lossy()
}

#[tokio::test]
async fn reports_a_supported_version() {
    let version = git().version().await.expect("git --version");
    assert!(version.is_supported(), "git {version} is too old");
}

#[tokio::test]
async fn passes_arguments_literally_without_a_shell() {
    let dir = tempfile::tempdir().expect("tempdir");
    let marker = dir.path().join("pwned");
    let hostile = [
        "a; b".to_string(),
        "$(touch pwned)".to_string(),
        "`touch pwned`".to_string(),
        "*".to_string(),
        "it's \"quoted\"".to_string(),
        format!("> {}", marker.display()),
    ];
    let out = read_in(dir.path())
        .args(["rev-parse", "--sq-quote"])
        .args(&hostile)
        .output(&CancellationToken::new())
        .await
        .expect("rev-parse --sq-quote");

    // --sq-quote echoes each argument it received, shell-quoted: one per input.
    let expected = hostile
        .iter()
        .map(|a| format!(" '{}'", a.replace('\'', "'\\''")))
        .collect::<String>();
    assert_eq!(stdout_of(&out).trim_end(), expected);
    assert!(!marker.exists(), "an argument was interpreted by a shell");
}

#[tokio::test]
async fn non_zero_exit_is_a_failure_with_stderr() {
    let dir = tempfile::tempdir().expect("tempdir");
    let err = read_in(dir.path())
        .args(["rev-parse", "--verify", "HEAD"])
        .output(&CancellationToken::new())
        .await
        .expect_err("not a repository");
    match err {
        GitError::Failed {
            args,
            exit_code,
            stderr,
            ..
        } => {
            assert_eq!(args, ["rev-parse", "--verify", "HEAD"]);
            assert_eq!(exit_code, Some(128));
            assert!(stderr.contains("not a git repository"), "{stderr}");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn accepted_exit_codes_are_not_failures() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = read_in(dir.path())
        .args(["-c", "alias.no=!exit 1", "no"])
        .accept_exit_code(1)
        .output(&CancellationToken::new())
        .await
        .expect("exit 1 accepted");
    assert_eq!(out.exit_code, Some(1));
}

#[tokio::test]
async fn sets_a_stable_environment() {
    let dir = tempfile::tempdir().expect("tempdir");
    let env_of = |cmd: GitCommand| async {
        let out = isolated(cmd)
            .args(["-c", "alias.showenv=!env", "showenv"])
            .output(&CancellationToken::new())
            .await
            .expect("env");
        stdout_of(&out)
    };

    let read_env = env_of(git().read(dir.path())).await;
    assert!(read_env.contains("\nLC_ALL=C\n") || read_env.starts_with("LC_ALL=C\n"));
    assert!(read_env.contains("GIT_TERMINAL_PROMPT=0"));
    assert!(read_env.contains("GIT_OPTIONAL_LOCKS=0"));

    let write_env = env_of(git().write(dir.path())).await;
    assert!(write_env.contains("GIT_TERMINAL_PROMPT=0"));
    assert!(!write_env.contains("GIT_OPTIONAL_LOCKS"));
}

#[tokio::test]
async fn disables_path_quoting() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = read_in(dir.path())
        .args(["config", "--get", "core.quotepath"])
        .output(&CancellationToken::new())
        .await
        .expect("config");
    assert_eq!(stdout_of(&out).trim(), "false");
}

#[tokio::test]
async fn writes_stdin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = read_in(dir.path())
        .args(["hash-object", "--stdin"])
        .stdin("hello\n")
        .output(&CancellationToken::new())
        .await
        .expect("hash-object");
    assert_eq!(
        stdout_of(&out).trim(),
        "ce013625030ba8dba906f756967f9e9ca394464a"
    );
}

#[tokio::test]
async fn streams_nul_separated_records_and_progress() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut records = Vec::new();
    let mut progress = Vec::new();
    let out = read_in(dir.path())
        .args([
            "-c",
            r#"alias.emit=!printf 'one\0two words\0ñ\0' && printf 'Step 50%%\rStep 100%%\ndone\n' >&2"#,
            "emit",
        ])
        .stream(
            0,
            &CancellationToken::new(),
            |r| records.push(String::from_utf8_lossy(r).into_owned()),
            |line| progress.push(line.to_owned()),
        )
        .await
        .expect("stream");
    assert_eq!(records, ["one", "two words", "ñ"]);
    assert_eq!(progress, ["Step 50%", "Step 100%", "done"]);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "Step 50%\rStep 100%\ndone\n");
}

/// Starts `sleep` as a grandchild of git (git → sh → sleep) and records its pid.
fn slow_command(dir: &Path) -> GitCommand {
    read_in(dir).env("PIDFILE", dir.join("sleep.pid")).args([
        "-c",
        r#"alias.slow=!echo $$ > "$PIDFILE"; exec sleep 30"#,
        "slow",
    ])
}

async fn wait_for_pid(dir: &Path) -> i32 {
    let file = dir.join("sleep.pid");
    for _ in 0..100 {
        if let Ok(text) = std::fs::read_to_string(&file)
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("grandchild never started");
}

#[cfg(unix)]
async fn assert_process_gone(pid: i32) {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    for _ in 0..100 {
        if kill(Pid::from_raw(pid), None).is_err() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("process {pid} survived cancellation");
}

#[tokio::test]
async fn cancelling_kills_the_whole_process_group() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cancel = CancellationToken::new();
    let started = Instant::now();

    let run = tokio::spawn({
        let cancel = cancel.clone();
        let cmd = slow_command(dir.path());
        async move { cmd.output(&cancel).await }
    });
    let grandchild = wait_for_pid(dir.path()).await;
    cancel.cancel();

    let result = run.await.expect("join");
    assert!(matches!(result, Err(GitError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    #[cfg(unix)]
    assert_process_gone(grandchild).await;
    let _ = grandchild;
}

#[tokio::test]
async fn times_out_long_commands() {
    let dir = tempfile::tempdir().expect("tempdir");
    let started = Instant::now();
    let result = slow_command(dir.path())
        .timeout(Duration::from_millis(300))
        .output(&CancellationToken::new())
        .await;
    assert!(
        matches!(result, Err(GitError::TimedOut(d)) if d == Duration::from_millis(300)),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    #[cfg(unix)]
    assert_process_gone(wait_for_pid(dir.path()).await).await;
}

#[tokio::test]
async fn already_cancelled_token_stops_immediately() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = slow_command(dir.path()).output(&cancel).await;
    assert!(matches!(result, Err(GitError::Cancelled)), "{result:?}");
}

#[tokio::test]
async fn missing_executable_is_a_spawn_error() {
    let result = Git::at("/nonexistent/git")
        .read(std::env::temp_dir())
        .arg("--version")
        .output(&CancellationToken::new())
        .await;
    assert!(matches!(result, Err(GitError::Spawn(_))), "{result:?}");
}

/// Keeps every event a [`CommandLog`] receives.
#[derive(Default)]
struct Recorder(std::sync::Mutex<Vec<CommandEvent>>);

impl CommandLog for Recorder {
    fn record(&self, event: CommandEvent) {
        self.0.lock().expect("recorder").push(event);
    }
}

#[tokio::test]
async fn logs_each_command_with_its_outcome_and_without_credentials() {
    let dir = tempfile::tempdir().expect("tempdir");
    let recorder = Arc::new(Recorder::default());
    let git = git().with_log(recorder.clone());

    isolated(git.read(dir.path()))
        .arg("--version")
        .output(&CancellationToken::new())
        .await
        .expect("version");
    let failed = isolated(git.read(dir.path()))
        .args(["ls-remote", "--", "file://ana:tok@/nonexistent/x.git"])
        .output(&CancellationToken::new())
        .await;
    assert!(failed.is_err());

    let events = recorder.0.lock().expect("recorder").clone();
    let [
        CommandEvent::Started {
            id: first,
            command: version,
            access: Access::Read,
            cwd,
            ..
        },
        CommandEvent::Finished {
            id: first_end,
            outcome: CommandOutcome::Succeeded,
            ..
        },
        CommandEvent::Started {
            id: second,
            command: ls_remote,
            ..
        },
        CommandEvent::Finished {
            id: second_end,
            outcome: CommandOutcome::Failed { code: Some(_) },
            ..
        },
    ] = events.as_slice()
    else {
        panic!("unexpected events: {events:?}");
    };
    assert_eq!((first, second), (first_end, second_end));
    assert_ne!(first, second);
    assert_eq!(cwd, dir.path());
    assert_eq!(
        version,
        "git -c core.quotepath=false -c color.ui=false --version"
    );
    assert!(!ls_remote.contains("tok"), "{ls_remote}");
    assert!(ls_remote.ends_with("ls-remote -- file://***@/nonexistent/x.git"));
}

#[tokio::test]
async fn logs_a_cancelled_command_as_cancelled() {
    let dir = tempfile::tempdir().expect("tempdir");
    let recorder = Arc::new(Recorder::default());
    let cancel = CancellationToken::new();
    cancel.cancel();
    let cmd = git().with_log(recorder.clone()).read(dir.path());
    let _ = isolated(cmd).arg("status").output(&cancel).await;
    let events = recorder.0.lock().expect("recorder").clone();
    assert!(
        matches!(
            events.last(),
            Some(CommandEvent::Finished {
                outcome: CommandOutcome::Cancelled,
                ..
            })
        ),
        "{events:?}"
    );
}
