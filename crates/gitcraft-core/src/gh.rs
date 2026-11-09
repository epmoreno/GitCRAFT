//! GitHub CLI (`gh`): the GitHub session GitCRAFT relies on. GitCRAFT never
//! reads or stores a token: it never runs `gh auth token` nor passes
//! `--show-token`. gh keeps the session in the macOS Keychain.
//!
//! Like git, gh runs with its arguments as an array (never a shell), with a
//! controlled environment and in its own process group, so cancelling kills
//! it and anything it started.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use crate::error::GhHttp;
use crate::platform::{Os, is_executable};
use crate::process_tree::ProcessTree;
use crate::redact::redact_credentials;
use crate::runner::log::{LogHandle, Pending, shell_line};
use crate::{Access, CancellationToken, CommandLog, CommandOutcome, ErrorKind, GitError, Result};

/// The only host GitCRAFT signs in to.
pub const GITHUB_HOST: &str = "github.com";

/// Environment variable that forces a specific gh executable (tests).
pub const GH_PATH_OVERRIDE: &str = "GITCRAFT_GH";

/// How long read-only checks may take. gh itself waits about 30 s without a
/// network before giving up.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// A located gh executable.
#[derive(Debug, Clone)]
pub struct Gh {
    exe: PathBuf,
    envs: Vec<(OsString, OsString)>,
    log: Option<LogHandle>,
    /// The repository these commands are about, if any.
    repo: Option<PathBuf>,
    /// Limit for read-only commands; those that change something run to the
    /// end (stopping them halfway would leave their effect unknown).
    check_timeout: Duration,
}

/// One account gh knows for github.com.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GhAccount {
    pub login: String,
    /// The one gh uses.
    pub active: bool,
    /// The token works (false: expired or revoked; sign in again).
    pub valid: bool,
    /// `https` or `ssh`: how git talks to GitHub for this account.
    pub git_protocol: String,
}

/// What `gh auth status --json hosts` reports, for github.com only.
#[derive(Debug, Deserialize)]
struct StatusJson {
    #[serde(default)]
    hosts: std::collections::BTreeMap<String, Vec<HostEntry>>,
}

#[derive(Debug, Deserialize)]
struct HostEntry {
    #[serde(default)]
    login: String,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    state: String,
    #[serde(default, rename = "gitProtocol")]
    git_protocol: String,
}

/// The github.com session, as far as it could be checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub accounts: Vec<GhAccount>,
    /// GitHub could not be reached: `accounts` says nothing about whether the
    /// session is valid. Never to be shown as "not signed in".
    pub unreachable: bool,
}

/// A repository just created on GitHub, from the API's JSON.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CreatedRepo {
    pub full_name: String,
    pub html_url: String,
    pub clone_url: String,
    pub ssh_url: String,
}

/// An HTTP answer from `gh api --include`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ApiResponse {
    status: u16,
    body: String,
}

impl ApiResponse {
    /// GitHub's validation errors name the field they are about.
    fn rejects_field(&self, field: &str) -> bool {
        #[derive(Deserialize)]
        struct Validation {
            #[serde(default)]
            errors: Vec<FieldError>,
        }
        #[derive(Deserialize)]
        struct FieldError {
            #[serde(default)]
            field: String,
        }
        serde_json::from_str::<Validation>(&self.body)
            .is_ok_and(|v| v.errors.iter().any(|e| e.field == field))
    }

    fn into_error(self, path: &str) -> GitError {
        GitError::GhFailed {
            args: vec!["api".into(), path.to_owned()],
            exit_code: Some(1),
            stderr: self.body,
            http: GhHttp::Status(self.status),
        }
    }
}

/// `HTTP/2.0 404 Not Found`, the headers, a blank line and the body.
fn parse_api_response(text: &str) -> Option<ApiResponse> {
    let mut status_line = text.lines().next()?.split(' ');
    if !status_line.next()?.starts_with("HTTP/") {
        return None;
    }
    let status = status_line.next()?.parse().ok()?;
    let body = text
        .split_once("\r\n\r\n")
        .or_else(|| text.split_once("\n\n"))
        .map_or("", |(_, body)| body);
    Some(ApiResponse {
        status,
        body: body.to_owned(),
    })
}

/// What the sign-in reports while it waits for the browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum LoginEvent {
    /// The one-time code to type at `url`.
    Code { code: String, url: String },
    /// Anything else gh said, as is (shown when no code could be read).
    Line { text: String },
}

impl Gh {
    /// `$GITCRAFT_GH`, then `$PATH`, then where each system usually installs
    /// it ([`Os::gh_locations`]); `None` when
    /// gh is not installed.
    pub fn discover() -> Option<Self> {
        if let Some(path) = std::env::var_os(GH_PATH_OVERRIDE).filter(|p| !p.is_empty()) {
            let path = PathBuf::from(path);
            return is_executable(&path).then(|| Self::at(path));
        }
        let os = Os::current();
        let exe = os.exe("gh");
        std::env::var_os("PATH")
            .iter()
            .flat_map(std::env::split_paths)
            .map(|dir| dir.join(&exe))
            .chain(os.gh_locations(&|var| std::env::var_os(var)))
            .find(|candidate| is_executable(candidate))
            .map(Self::at)
    }

    pub fn at(exe: impl Into<PathBuf>) -> Self {
        Self {
            exe: exe.into(),
            envs: Vec::new(),
            log: None,
            repo: None,
            check_timeout: CHECK_TIMEOUT,
        }
    }

    /// Changes [`CHECK_TIMEOUT`] (tests).
    pub fn with_check_timeout(mut self, limit: Duration) -> Self {
        self.check_timeout = limit;
        self
    }

    /// Reports every command to `log`, like [`crate::Git::with_log`].
    pub fn with_log(mut self, log: Arc<dyn CommandLog>) -> Self {
        self.log = Some(LogHandle(log));
        self
    }

    /// The commands are about the repository at `repo`: they run there and
    /// the history shows them with it. Otherwise they belong to no repository
    /// and the history records an empty folder.
    pub fn in_repo(mut self, repo: impl Into<PathBuf>) -> Self {
        self.repo = Some(repo.into());
        self
    }

    /// Records the start of a command in the history.
    fn pending(&self, args: &[&str], access: Access) -> Option<Pending> {
        let log = self.log.clone()?;
        let line = shell_line(std::iter::once("gh").chain(args.iter().copied()));
        let command = redact_credentials(&line).into_owned();
        let cwd = self.repo.clone().unwrap_or_default();
        Some(Pending::start(log, cwd, command, access))
    }

    pub fn exe(&self) -> &Path {
        &self.exe
    }

    /// Sets `key=value` on every gh command (tests isolate gh's config).
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    fn command(&self, args: &[&str]) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.exe);
        cmd.args(args)
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("GH_NO_EXTENSION_UPDATE_NOTIFIER", "1")
            .env("GH_SPINNER_DISABLED", "1")
            .env("NO_COLOR", "1")
            .env("CLICOLOR", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (key, value) in &self.envs {
            cmd.env(key, value);
        }
        if let Some(repo) = &self.repo {
            cmd.current_dir(repo);
        }
        crate::process_tree::prepare(&mut cmd);
        cmd
    }

    /// Runs gh to the end, recording it; a non-zero exit is an error with its
    /// stderr.
    async fn run(
        &self,
        args: &[&str],
        access: Access,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let pending = self.pending(args, access);
        let result = self.run_unlogged(args, access, cancel).await;
        finish(pending, CommandOutcome::of(&result), &result);
        result
    }

    async fn run_unlogged(
        &self,
        args: &[&str],
        access: Access,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let output = self.output(args, access, cancel).await?;
        if !output.status.success() {
            return Err(failed(args, output.status.code(), &output.stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    async fn output(
        &self,
        args: &[&str],
        access: Access,
        cancel: &CancellationToken,
    ) -> Result<std::process::Output> {
        let child = self.command(args).spawn().map_err(spawn_error)?;
        let tree = ProcessTree::adopt(&child);
        let limit = async {
            if access == Access::Read {
                tokio::time::sleep(self.check_timeout).await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! {
            output = child.wait_with_output() => Ok(output?),
            () = cancel.cancelled() => {
                tree.terminate();
                Err(GitError::Cancelled)
            }
            () = limit => {
                tree.terminate();
                Err(GitError::TimedOut(self.check_timeout))
            }
        }
    }

    /// `gh --version`'s first line ("gh version 2.62.0 (2024-11-14)").
    pub async fn version(&self, cancel: &CancellationToken) -> Result<String> {
        let out = self.run(&["--version"], Access::Read, cancel).await?;
        Ok(out.lines().next().unwrap_or_default().trim().to_owned())
    }

    /// The accounts gh has for github.com (empty: no session).
    pub async fn accounts(&self, cancel: &CancellationToken) -> Result<Vec<GhAccount>> {
        let out = self
            .run(
                &[
                    "auth",
                    "status",
                    "--hostname",
                    GITHUB_HOST,
                    "--json",
                    "hosts",
                ],
                Access::Read,
                cancel,
            )
            .await?;
        parse_accounts(&out)
    }

    /// The session for github.com, telling "no valid session" apart from "could
    /// not check". Without a network `gh auth status` waits and then reports
    /// the token as not working, like a revoked one, so a token that does not
    /// show as working is asked about directly: only an HTTP answer from
    /// GitHub says it is invalid. All of it within [`CHECK_TIMEOUT`].
    pub async fn session(&self, cancel: &CancellationToken) -> Result<Session> {
        let unreachable = |accounts| Session {
            accounts,
            unreachable: true,
        };
        let start = Instant::now();
        let mut accounts = match self.accounts(cancel).await {
            Ok(accounts) => accounts,
            Err(GitError::TimedOut(_)) => return Ok(unreachable(Vec::new())),
            Err(err) => return Err(err),
        };
        let checked = |accounts| Session {
            accounts,
            unreachable: false,
        };
        let Some(active) = accounts.iter().position(|a| a.active && !a.valid) else {
            return Ok(checked(accounts));
        };
        let mut probe = self.clone();
        probe.check_timeout = self.check_timeout.saturating_sub(start.elapsed());
        match probe.api(&["user"], Access::Read, &[401], cancel).await {
            Ok(response) if response.status == 200 => {
                if let Some(account) = accounts.get_mut(active) {
                    account.valid = true;
                }
                Ok(checked(accounts))
            }
            Ok(response) if response.status == 401 => Ok(checked(accounts)),
            // GitHub answered something that says nothing about the token.
            Ok(_) | Err(GitError::TimedOut(_)) => Ok(unreachable(accounts)),
            Err(err) => match err.kind() {
                ErrorKind::GhUnreachable => Ok(unreachable(accounts)),
                ErrorKind::GhNotLoggedIn => Ok(checked(accounts)),
                _ => Err(err),
            },
        }
    }

    /// Signs in to github.com through the browser (device flow): `on_event`
    /// gets the one-time code as soon as gh prints it. Returns when the code
    /// was accepted in the browser; cancel to give up.
    pub async fn login(
        &self,
        mut on_event: impl FnMut(LoginEvent) + Send,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let args = [
            "auth",
            "login",
            "--hostname",
            GITHUB_HOST,
            "--web",
            "--git-protocol",
            "https",
            "--skip-ssh-key",
        ];
        // Only the arguments are recorded: never the code gh prints.
        let pending = self.pending(&args, Access::Write);
        let result = self.login_unlogged(&args, &mut on_event, cancel).await;
        finish(pending, CommandOutcome::of(&result), &result);
        result
    }

    async fn login_unlogged(
        &self,
        args: &[&str],
        on_event: &mut (impl FnMut(LoginEvent) + Send),
        cancel: &CancellationToken,
    ) -> Result<()> {
        let mut child = self.command(args).spawn().map_err(spawn_error)?;
        let tree = ProcessTree::adopt(&child);
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| GitError::Parse("no stderr".into()))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| GitError::Parse("no stdout".into()))?;
        let mut said = String::new();
        let mut lines = BufReader::new(stderr).lines();
        let read = async {
            while let Some(line) = lines.next_line().await? {
                said.push_str(&line);
                said.push('\n');
                on_event(login_event(&line));
            }
            let mut rest = String::new();
            stdout.read_to_string(&mut rest).await?;
            child.wait().await
        };
        let status = tokio::select! {
            status = read => status?,
            () = cancel.cancelled() => {
                tree.terminate();
                return Err(GitError::Cancelled);
            }
        };
        if !status.success() {
            return Err(failed(args, status.code(), said.as_bytes()));
        }
        Ok(())
    }

    /// Runs `gh api --include …`. An HTTP answer is returned whatever its
    /// status (gh exits with 1 for 4xx and 5xx); only no answer is an error.
    /// The history counts 2xx and `expected` as success.
    async fn api(
        &self,
        args: &[&str],
        access: Access,
        expected: &[u16],
        cancel: &CancellationToken,
    ) -> Result<ApiResponse> {
        let mut full = vec!["api", "--include"];
        full.extend_from_slice(args);
        let pending = self.pending(&full, access);
        let result = self.api_unlogged(&full, access, cancel).await;
        let outcome = match &result {
            Ok(r) if (200..300).contains(&r.status) || expected.contains(&r.status) => {
                CommandOutcome::Succeeded
            }
            Ok(_) => CommandOutcome::Failed { code: Some(1) },
            Err(_) => CommandOutcome::of(&result),
        };
        finish(pending, outcome, &result);
        result
    }

    async fn api_unlogged(
        &self,
        full: &[&str],
        access: Access,
        cancel: &CancellationToken,
    ) -> Result<ApiResponse> {
        let output = self.output(full, access, cancel).await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        match parse_api_response(&stdout) {
            // Exit code 4: gh refused to call without a session.
            Some(response) if output.status.code() != Some(4) => Ok(response),
            _ => Err(GitError::GhFailed {
                args: full.iter().map(|a| (*a).to_owned()).collect(),
                exit_code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                http: GhHttp::NoResponse,
            }),
        }
    }

    /// Whether `owner/name` exists (and the session can see it): 200 or 404.
    pub async fn repo_exists(
        &self,
        owner: &str,
        name: &str,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        let path = format!("repos/{owner}/{name}");
        let response = self
            .api(&[&path], Access::Read, &[404], cancel)
            .await
            .map_err(|err| match err {
                GitError::TimedOut(_) => GitError::GhUnreachable,
                other => other,
            })?;
        match response.status {
            200 => Ok(true),
            404 => Ok(false),
            _ => Err(response.into_error(&path)),
        }
    }

    /// Creates an empty repository `name` in the signed-in personal account
    /// (`POST user/repos`). If the name got taken since it was checked, says
    /// so with [`GitError::GhRepoExists`].
    pub async fn create_repo(
        &self,
        name: &str,
        description: Option<&str>,
        private: bool,
        cancel: &CancellationToken,
    ) -> Result<CreatedRepo> {
        // `key=value` in one argument: a value starting with `-` can never be
        // read as an option.
        let name_field = format!("name={name}");
        let private_field = format!("private={private}");
        let description_field = description
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(|d| format!("description={d}"));
        let mut args = vec![
            "-X",
            "POST",
            "user/repos",
            "-f",
            &name_field,
            "-F",
            &private_field,
        ];
        if let Some(field) = &description_field {
            args.extend(["-f", field]);
        }
        let response = self.api(&args, Access::Write, &[], cancel).await?;
        match response.status {
            201 => serde_json::from_str(&response.body)
                .map_err(|e| GitError::Parse(format!("POST user/repos: {e}"))),
            422 if response.rejects_field("name") => Err(GitError::GhRepoExists(name.to_owned())),
            _ => Err(response.into_error("user/repos")),
        }
    }

    /// Signs `login` out of github.com (gh forgets its token).
    pub async fn logout(&self, login: &str, cancel: &CancellationToken) -> Result<()> {
        self.run(
            &["auth", "logout", "--hostname", GITHUB_HOST, "--user", login],
            Access::Write,
            cancel,
        )
        .await
        .map(drop)
    }

    /// Makes git ask gh for github.com credentials (writes the credential
    /// helper for github.com in the global git config).
    pub async fn setup_git(&self, cancel: &CancellationToken) -> Result<()> {
        self.run(
            &["auth", "setup-git", "--hostname", GITHUB_HOST],
            Access::Write,
            cancel,
        )
        .await
        .map(drop)
    }
}

/// Records the end of a command, unless it was cancelled: dropping `pending`
/// records that.
fn finish<T>(pending: Option<Pending>, outcome: CommandOutcome, result: &Result<T>) {
    if !matches!(result, Err(GitError::Cancelled))
        && let Some(pending) = pending
    {
        pending.finish(outcome);
    }
}

fn parse_accounts(json: &str) -> Result<Vec<GhAccount>> {
    let status: StatusJson =
        serde_json::from_str(json).map_err(|e| GitError::Parse(format!("gh auth status: {e}")))?;
    Ok(status
        .hosts
        .get(GITHUB_HOST)
        .map(|entries| {
            entries
                .iter()
                .filter(|e| !e.login.is_empty())
                .map(|e| GhAccount {
                    login: e.login.clone(),
                    active: e.active,
                    valid: e.state == "success",
                    git_protocol: if e.git_protocol.is_empty() {
                        "https".to_owned()
                    } else {
                        e.git_protocol.clone()
                    },
                })
                .collect()
        })
        .unwrap_or_default())
}

/// gh writes the code as `XXXX-XXXX` (letters and digits) and the device
/// page URL in its own lines; anything else is passed on as text.
fn login_event(line: &str) -> LoginEvent {
    let code = line
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .find(|word| is_device_code(word));
    match code {
        Some(code) => LoginEvent::Code {
            code: code.to_owned(),
            url: format!("https://{GITHUB_HOST}/login/device"),
        },
        None => LoginEvent::Line {
            text: line.trim().to_owned(),
        },
    }
}

fn is_device_code(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() == 9
        && bytes[4] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || b.is_ascii_uppercase() || b.is_ascii_digit())
}

fn failed(args: &[&str], exit_code: Option<i32>, stderr: &[u8]) -> GitError {
    GitError::GhFailed {
        args: args.iter().map(|a| (*a).to_owned()).collect(),
        exit_code,
        stderr: String::from_utf8_lossy(stderr).into_owned(),
        http: GhHttp::NotApi,
    }
}

fn spawn_error(err: std::io::Error) -> GitError {
    if err.kind() == std::io::ErrorKind::NotFound {
        GitError::GhNotFound
    } else {
        GitError::Spawn(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_github_accounts_from_the_json_status() {
        let json = r#"{"hosts":{"github.com":[
            {"state":"success","active":true,"host":"github.com","login":"ana","tokenSource":"keyring","scopes":"repo","gitProtocol":"https"},
            {"state":"error","active":false,"host":"github.com","login":"ana-old"}
        ],"github.example.com":[{"state":"success","active":true,"login":"work"}]}}"#;
        assert_eq!(
            parse_accounts(json).expect("parse"),
            [
                GhAccount {
                    login: "ana".into(),
                    active: true,
                    valid: true,
                    git_protocol: "https".into(),
                },
                GhAccount {
                    login: "ana-old".into(),
                    active: false,
                    valid: false,
                    git_protocol: "https".into(),
                },
            ]
        );
        assert_eq!(parse_accounts(r#"{"hosts":{}}"#).expect("parse"), []);
        assert!(parse_accounts("You are not logged in").is_err());
    }

    #[test]
    fn reads_the_status_and_body_of_a_real_api_answer() {
        let real = include_str!("../tests/fixtures/gh_api_404.txt");
        let response = parse_api_response(real).expect("response");
        assert_eq!(response.status, 404);
        assert!(response.body.starts_with('{'), "{}", response.body);

        let crlf = parse_api_response("HTTP/2.0 201 Created\r\nA: b\r\n\r\n{}").expect("crlf");
        assert_eq!((crlf.status, crlf.body.as_str()), (201, "{}"));
        assert_eq!(parse_api_response("gh: something else"), None);
        assert_eq!(parse_api_response(""), None);
    }

    #[test]
    fn finds_the_one_time_code_in_gh_output() {
        assert_eq!(
            login_event("! One-time code (AB12-CD34) copied to clipboard"),
            LoginEvent::Code {
                code: "AB12-CD34".into(),
                url: "https://github.com/login/device".into()
            }
        );
        assert_eq!(
            login_event("! First copy your one-time code: 9Z9Z-0A0A"),
            LoginEvent::Code {
                code: "9Z9Z-0A0A".into(),
                url: "https://github.com/login/device".into()
            }
        );
        assert_eq!(
            login_event(
                "Open this URL to continue in your web browser: https://github.com/login/device"
            ),
            LoginEvent::Line {
                text:
                    "Open this URL to continue in your web browser: https://github.com/login/device"
                        .into()
            }
        );
        assert!(!is_device_code("ab12-cd34"));
        assert!(!is_device_code("ABCDE-FGH"));
    }
}
