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

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use crate::{CancellationToken, GitError, Result};

/// The only host GitCRAFT signs in to.
pub const GITHUB_HOST: &str = "github.com";

/// Environment variable that forces a specific gh executable (tests).
pub const GH_PATH_OVERRIDE: &str = "GITCRAFT_GH";

/// Checked after `PATH` (apps opened from Finder get a minimal one).
const FALLBACK_LOCATIONS: &[&str] = &["/opt/homebrew/bin/gh", "/usr/local/bin/gh"];

/// A located gh executable.
#[derive(Debug, Clone)]
pub struct Gh {
    exe: PathBuf,
    envs: Vec<(OsString, OsString)>,
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
    /// `$GITCRAFT_GH`, then `$PATH`, then the Homebrew locations; `None` when
    /// gh is not installed.
    pub fn discover() -> Option<Self> {
        if let Some(path) = std::env::var_os(GH_PATH_OVERRIDE).filter(|p| !p.is_empty()) {
            let path = PathBuf::from(path);
            return is_executable(&path).then(|| Self::at(path));
        }
        std::env::var_os("PATH")
            .iter()
            .flat_map(std::env::split_paths)
            .map(|dir| dir.join("gh"))
            .chain(FALLBACK_LOCATIONS.iter().map(PathBuf::from))
            .find(|candidate| is_executable(candidate))
            .map(Self::at)
    }

    pub fn at(exe: impl Into<PathBuf>) -> Self {
        Self {
            exe: exe.into(),
            envs: Vec::new(),
        }
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
        #[cfg(unix)]
        cmd.process_group(0);
        cmd
    }

    /// Runs gh to the end; a non-zero exit is an error with its stderr.
    async fn run(&self, args: &[&str], cancel: &CancellationToken) -> Result<String> {
        let child = self.command(args).spawn().map_err(spawn_error)?;
        let output = tokio::select! {
            output = child.wait_with_output() => output?,
            () = cancel.cancelled() => return Err(GitError::Cancelled),
        };
        if !output.status.success() {
            return Err(failed(args, output.status.code(), &output.stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// `gh --version`'s first line ("gh version 2.62.0 (2024-11-14)").
    pub async fn version(&self, cancel: &CancellationToken) -> Result<String> {
        let out = self.run(&["--version"], cancel).await?;
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
                cancel,
            )
            .await?;
        parse_accounts(&out)
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
        let mut child = self.command(&args).spawn().map_err(spawn_error)?;
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
                kill_group(&child);
                return Err(GitError::Cancelled);
            }
        };
        if !status.success() {
            return Err(failed(&args, status.code(), said.as_bytes()));
        }
        Ok(())
    }

    /// Whether `owner/name` exists (and the session can see it).
    pub async fn repo_exists(
        &self,
        owner: &str,
        name: &str,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        let full = format!("{owner}/{name}");
        match self
            .run(&["repo", "view", &full, "--json", "name"], cancel)
            .await
        {
            Ok(_) => Ok(true),
            Err(GitError::GhFailed { stderr, .. })
                if stderr.contains("Could not resolve to a Repository") =>
            {
                Ok(false)
            }
            Err(err) => Err(err),
        }
    }

    /// Creates an empty repository `name` in the signed-in account and
    /// returns its web URL. `name` must be valid (see
    /// `ops::publish::valid_repo_name`): it never starts with `-`.
    pub async fn create_repo(
        &self,
        name: &str,
        description: Option<&str>,
        private: bool,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let visibility = if private { "--private" } else { "--public" };
        // `--description=…` in one argument: a description starting with
        // `-` can never be read as another option.
        let description = description
            .filter(|d| !d.trim().is_empty())
            .map(|d| format!("--description={}", d.trim()));
        let mut args = vec!["repo", "create", name, visibility];
        if let Some(d) = &description {
            args.push(d);
        }
        let out = self.run(&args, cancel).await?;
        out.lines()
            .map(str::trim)
            .rfind(|line| line.contains("://"))
            .map(|url| url.trim_end_matches('/').to_owned())
            .ok_or_else(|| GitError::Parse(format!("gh repo create: {out:?}")))
    }

    /// Signs `login` out of github.com (gh forgets its token).
    pub async fn logout(&self, login: &str, cancel: &CancellationToken) -> Result<()> {
        self.run(
            &["auth", "logout", "--hostname", GITHUB_HOST, "--user", login],
            cancel,
        )
        .await
        .map(drop)
    }

    /// Makes git ask gh for github.com credentials (writes the credential
    /// helper for github.com in the global git config).
    pub async fn setup_git(&self, cancel: &CancellationToken) -> Result<()> {
        self.run(&["auth", "setup-git", "--hostname", GITHUB_HOST], cancel)
            .await
            .map(drop)
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
    }
}

fn spawn_error(err: std::io::Error) -> GitError {
    if err.kind() == std::io::ErrorKind::NotFound {
        GitError::GhNotFound
    } else {
        GitError::Spawn(err)
    }
}

#[cfg(unix)]
fn kill_group(child: &tokio::process::Child) {
    if let Some(pid) = child.id().and_then(|id| i32::try_from(id).ok()) {
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(pid),
            nix::sys::signal::Signal::SIGTERM,
        );
    }
}

#[cfg(not(unix))]
fn kill_group(_child: &tokio::process::Child) {}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
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
