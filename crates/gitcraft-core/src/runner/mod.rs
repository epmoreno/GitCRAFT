//! Runs the system `git` binary.
//!
//! Every invocation goes through [`GitCommand`]: arguments are passed as an
//! array (never through a shell), the environment is fixed so output is
//! parseable, and long operations can be cancelled or time out. Cancelling
//! kills git's whole process group, including hooks and `ssh` children.
//! A [`CommandLog`] attached to [`Git`] sees every command start and end.

mod discover;
pub(crate) mod log;
mod split;

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Child;
use tokio_util::sync::CancellationToken;

pub use discover::{GIT_PATH_OVERRIDE, GitVersion, find_git};
pub use log::{CommandEvent, CommandLog, CommandOutcome};
use log::{LogHandle, Pending, shell_line};
use split::Splitter;

use crate::error::{GitError, Result};
use crate::redact::redact_credentials;

/// Config overrides applied to every invocation.
const BASE_ARGS: &[&str] = &["-c", "core.quotepath=false", "-c", "color.ui=false"];

/// Inherited variables that would redirect git to another repository or index.
const BLOCKED_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
];

/// Added to the end of git's `PATH` when missing: apps launched from Finder
/// get a minimal `PATH`, and git runs helpers (credential helpers such as
/// `gh`, git-lfs, ssh) that are often installed with Homebrew.
const EXTRA_PATH_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

/// Time git gets to exit after SIGTERM before the group is SIGKILLed.
const KILL_GRACE: Duration = Duration::from_secs(2);

/// How much of stdout is kept for error reports.
const STDOUT_TAIL_BYTES: usize = 4096;

/// Whether a command only reads the repository or may modify it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Runs with `GIT_OPTIONAL_LOCKS=0` so it never competes for `index.lock`.
    Read,
    Write,
}

/// A located git executable.
#[derive(Debug, Clone)]
pub struct Git {
    exe: PathBuf,
    log: Option<LogHandle>,
    /// Environment set on every command (tests isolate git's global config).
    envs: Vec<(OsString, OsString)>,
}

impl Git {
    pub fn discover() -> Result<Self> {
        find_git().map(Self::at).ok_or(GitError::GitNotFound)
    }

    pub fn at(exe: impl Into<PathBuf>) -> Self {
        Self {
            exe: exe.into(),
            log: None,
            envs: Vec::new(),
        }
    }

    /// Sets `key=value` on every command run through this `Git`.
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    /// Reports every command run through this `Git` (and its clones made
    /// afterwards) to `log`.
    pub fn with_log(mut self, log: Arc<dyn CommandLog>) -> Self {
        self.log = Some(LogHandle(log));
        self
    }

    pub fn exe(&self) -> &Path {
        &self.exe
    }

    /// Where this `Git` reports its commands (`Gh` reports to the same place).
    pub fn command_log(&self) -> Option<Arc<dyn CommandLog>> {
        self.log.as_ref().map(|handle| Arc::clone(&handle.0))
    }

    /// A command that only reads the repository at `cwd`.
    pub fn read(&self, cwd: impl Into<PathBuf>) -> GitCommand {
        GitCommand::new(self, cwd.into(), Access::Read)
    }

    /// A command that may modify the repository at `cwd`.
    pub fn write(&self, cwd: impl Into<PathBuf>) -> GitCommand {
        GitCommand::new(self, cwd.into(), Access::Write)
    }

    pub async fn version(&self) -> Result<GitVersion> {
        let out = self
            .read(std::env::temp_dir())
            .arg("--version")
            .output(&CancellationToken::new())
            .await?;
        let text = out.stdout_lossy();
        GitVersion::parse(&text).ok_or_else(|| GitError::Parse(format!("git --version: {text}")))
    }
}

/// Result of a finished git process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub exit_code: Option<i32>,
    /// Full stdout for [`GitCommand::output`]; empty for [`GitCommand::stream`].
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl GitOutput {
    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// A single git invocation. Build it with [`Git::read`] or [`Git::write`].
#[derive(Debug, Clone)]
pub struct GitCommand {
    exe: PathBuf,
    log: Option<LogHandle>,
    cwd: PathBuf,
    access: Access,
    args: Vec<OsString>,
    envs: Vec<(OsString, OsString)>,
    stdin: Option<Vec<u8>>,
    timeout: Option<Duration>,
    ok_exit_codes: Vec<i32>,
}

struct RunResult {
    status: ExitStatus,
    stderr: Vec<u8>,
    stdout_tail: Vec<u8>,
}

enum Outcome {
    Finished(io::Result<RunResult>),
    Cancelled,
    TimedOut(Duration),
}

impl GitCommand {
    fn new(git: &Git, cwd: PathBuf, access: Access) -> Self {
        Self {
            exe: git.exe.clone(),
            log: git.log.clone(),
            cwd,
            access,
            args: Vec::new(),
            envs: git.envs.clone(),
            stdin: None,
            timeout: None,
            ok_exit_codes: vec![0],
        }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    /// Bytes written to git's stdin (stdin is closed when omitted).
    pub fn stdin(mut self, input: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(input.into());
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Treats `code` as success too (e.g. `merge-base --is-ancestor` exits 1 for "no").
    pub fn accept_exit_code(mut self, code: i32) -> Self {
        self.ok_exit_codes.push(code);
        self
    }

    pub fn access(&self) -> Access {
        self.access
    }

    /// Arguments as shown in logs and error reports (without the fixed base args).
    pub fn display_args(&self) -> Vec<String> {
        self.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// The whole command line as run (`git`, the fixed base args, then the
    /// arguments), quoted for a shell, with credentials in URLs redacted.
    pub fn command_line(&self) -> String {
        let args: Vec<String> = self.display_args();
        let line = shell_line(
            std::iter::once("git")
                .chain(BASE_ARGS.iter().copied())
                .chain(args.iter().map(String::as_str)),
        );
        redact_credentials(&line).into_owned()
    }

    /// Runs git and collects stdout.
    pub async fn output(self, cancel: &CancellationToken) -> Result<GitOutput> {
        self.output_with_progress(cancel, |_| {}).await
    }

    /// Runs git, collects stdout and reports each stderr progress line.
    pub async fn output_with_progress(
        self,
        cancel: &CancellationToken,
        on_progress: impl FnMut(&str),
    ) -> Result<GitOutput> {
        let mut stdout = Vec::new();
        let mut out = self
            .run(cancel, |chunk| stdout.extend_from_slice(chunk), on_progress)
            .await?;
        out.stdout = stdout;
        Ok(out)
    }

    /// Runs git and hands each `delimiter`-terminated stdout record to
    /// `on_record` as soon as it arrives.
    pub async fn stream(
        self,
        delimiter: u8,
        cancel: &CancellationToken,
        mut on_record: impl FnMut(&[u8]),
        on_progress: impl FnMut(&str),
    ) -> Result<GitOutput> {
        // A fn pointer cannot capture `delimiter`, so match the common cases.
        let is_delim: fn(u8) -> bool = match delimiter {
            0 => |b| b == 0,
            b'\n' => |b| b == b'\n',
            other => {
                return Err(GitError::Parse(format!(
                    "unsupported record delimiter {other:#04x}"
                )));
            }
        };
        let mut splitter = Splitter::new(is_delim);
        let out = self
            .run(
                cancel,
                |chunk| splitter.push(chunk, &mut on_record),
                on_progress,
            )
            .await?;
        splitter.finish(&mut on_record);
        Ok(out)
    }

    /// The process to spawn: fixed base args and environment, no shell.
    fn build(&self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.exe);
        cmd.current_dir(&self.cwd)
            .args(BASE_ARGS)
            .args(&self.args)
            .env("LC_ALL", "C")
            .env("GIT_TERMINAL_PROMPT", "0");
        if let Some(path) = child_path(std::env::var_os("PATH")) {
            cmd.env("PATH", path);
        }
        for key in BLOCKED_ENV {
            cmd.env_remove(key);
        }
        if self.access == Access::Read {
            cmd.env("GIT_OPTIONAL_LOCKS", "0");
        }
        cmd.envs(self.envs.iter().map(|(k, v)| (k, v)))
            .stdin(if self.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Own process group, so cancelling also reaches hooks and ssh.
        // Windows (future): use a Job Object instead.
        #[cfg(unix)]
        cmd.process_group(0);
        cmd
    }

    async fn run(
        self,
        cancel: &CancellationToken,
        on_stdout: impl FnMut(&[u8]),
        on_progress: impl FnMut(&str),
    ) -> Result<GitOutput> {
        let Some(log) = self.log.clone() else {
            return self.run_unlogged(cancel, on_stdout, on_progress).await;
        };
        let pending = Pending::start(log, self.cwd.clone(), self.command_line(), self.access);
        let result = self.run_unlogged(cancel, on_stdout, on_progress).await;
        pending.finish(CommandOutcome::of(&result));
        result
    }

    async fn run_unlogged(
        mut self,
        cancel: &CancellationToken,
        mut on_stdout: impl FnMut(&[u8]),
        mut on_progress: impl FnMut(&str),
    ) -> Result<GitOutput> {
        let mut child = self.build().spawn().map_err(GitError::Spawn)?;
        let pid = child.id();
        let (Some(mut stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take())
        else {
            return Err(GitError::Io(io::Error::other("git stdio was not captured")));
        };
        let stdin_pipe = child.stdin.take();
        let input = self.stdin.take();
        let timeout = self.timeout;

        let outcome = {
            let io = async {
                let write_stdin = async {
                    if let (Some(mut pipe), Some(data)) = (stdin_pipe, input) {
                        pipe.write_all(&data).await?;
                    }
                    Ok::<_, io::Error>(())
                };
                let read_stdout = async {
                    let mut tail = Vec::new();
                    let mut buf = vec![0u8; 64 * 1024];
                    loop {
                        let n = stdout.read(&mut buf).await?;
                        if n == 0 {
                            break;
                        }
                        on_stdout(&buf[..n]);
                        tail.extend_from_slice(&buf[..n]);
                        if tail.len() > 2 * STDOUT_TAIL_BYTES {
                            tail.drain(..tail.len() - STDOUT_TAIL_BYTES);
                        }
                    }
                    let keep_from = tail.len().saturating_sub(STDOUT_TAIL_BYTES);
                    tail.drain(..keep_from);
                    Ok::<_, io::Error>(tail)
                };
                let read_stderr = async {
                    let mut all = Vec::new();
                    let mut lines = Splitter::new(|b| b == b'\n' || b == b'\r');
                    let mut emit = |line: &[u8]| {
                        if !line.is_empty() {
                            on_progress(&String::from_utf8_lossy(line));
                        }
                    };
                    let mut buf = [0u8; 4096];
                    loop {
                        let n = stderr.read(&mut buf).await?;
                        if n == 0 {
                            break;
                        }
                        all.extend_from_slice(&buf[..n]);
                        lines.push(&buf[..n], &mut emit);
                    }
                    lines.finish(&mut emit);
                    Ok::<_, io::Error>(all)
                };

                let (written, stdout_tail, stderr) =
                    tokio::join!(write_stdin, read_stdout, read_stderr);
                // git may exit without reading all of stdin; that is not our error.
                if let Err(err) = written
                    && err.kind() != io::ErrorKind::BrokenPipe
                {
                    return Err(err);
                }
                let (stdout_tail, stderr) = (stdout_tail?, stderr?);
                let status = child.wait().await?;
                Ok(RunResult {
                    status,
                    stderr,
                    stdout_tail,
                })
            };
            let deadline = async {
                match timeout {
                    Some(limit) => tokio::time::sleep(limit).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                result = io => Outcome::Finished(result),
                () = cancel.cancelled() => Outcome::Cancelled,
                () = deadline => Outcome::TimedOut(timeout.unwrap_or_default()),
            }
        };

        let result = match outcome {
            Outcome::Finished(Ok(result)) => result,
            Outcome::Finished(Err(err)) => {
                terminate(&mut child, pid).await;
                return Err(GitError::Io(err));
            }
            Outcome::Cancelled => {
                terminate(&mut child, pid).await;
                return Err(GitError::Cancelled);
            }
            Outcome::TimedOut(limit) => {
                terminate(&mut child, pid).await;
                return Err(GitError::TimedOut(limit));
            }
        };

        let exit_code = result.status.code();
        let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
        if !exit_code.is_some_and(|code| self.ok_exit_codes.contains(&code)) {
            return Err(GitError::Failed {
                args: self.display_args(),
                exit_code,
                stderr,
                stdout_tail: String::from_utf8_lossy(&result.stdout_tail).into_owned(),
            });
        }
        Ok(GitOutput {
            exit_code,
            stdout: Vec::new(),
            stderr,
        })
    }
}

/// `path` with [`EXTRA_PATH_DIRS`] appended (those not already in it), so the
/// user's own order still wins.
fn child_path(path: Option<OsString>) -> Option<OsString> {
    let mut dirs: Vec<PathBuf> = path
        .as_deref()
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    for extra in EXTRA_PATH_DIRS {
        if !dirs.iter().any(|d| d == Path::new(extra)) {
            dirs.push(PathBuf::from(extra));
        }
    }
    std::env::join_paths(dirs).ok()
}

/// Asks git's process group to stop, then forces it after [`KILL_GRACE`].
async fn terminate(child: &mut Child, pid: Option<u32>) {
    signal_group(pid, GroupSignal::Terminate);
    if tokio::time::timeout(KILL_GRACE, child.wait())
        .await
        .is_err()
    {
        signal_group(pid, GroupSignal::Kill);
        // Also covers platforms without process groups.
        let _ = child.kill().await;
    }
}

enum GroupSignal {
    Terminate,
    Kill,
}

#[cfg(unix)]
fn signal_group(pid: Option<u32>, signal: GroupSignal) {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    let Some(pgid) = pid.and_then(|p| i32::try_from(p).ok()) else {
        return;
    };
    let signal = match signal {
        GroupSignal::Terminate => Signal::SIGTERM,
        GroupSignal::Kill => Signal::SIGKILL,
    };
    // ESRCH just means the group is already gone.
    let _ = killpg(Pid::from_raw(pgid), signal);
}

#[cfg(not(unix))]
fn signal_group(_pid: Option<u32>, _signal: GroupSignal) {
    // Windows: Job Objects will replace process groups (future phase).
    // Until then `terminate` falls back to killing the direct child.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(cmd: &tokio::process::Command, key: &str) -> Option<Option<String>> {
        cmd.as_std()
            .get_envs()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
    }

    #[test]
    fn prepends_base_args_before_user_args() {
        let cmd = Git::at("git")
            .read("/repo")
            .args(["log", "--", "-weird"])
            .build();
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "-c",
                "core.quotepath=false",
                "-c",
                "color.ui=false",
                "log",
                "--",
                "-weird"
            ]
        );
        assert_eq!(cmd.as_std().get_program(), "git");
        assert_eq!(cmd.as_std().get_current_dir(), Some(Path::new("/repo")));
    }

    #[test]
    fn command_line_is_exact_and_redacted() {
        let cmd = Git::at("/usr/bin/git").write("/repo").args([
            "clone",
            "--",
            "https://ana:tok@example.com/a b.git",
            "dest",
        ]);
        assert_eq!(
            cmd.command_line(),
            "git -c core.quotepath=false -c color.ui=false clone -- \
             'https://***@example.com/a b.git' dest"
        );
    }

    #[test]
    fn helpers_are_found_when_launched_from_finder() {
        let finder = child_path(Some("/usr/bin:/bin:/usr/sbin:/sbin".into()));
        assert_eq!(
            finder.as_deref(),
            Some(std::ffi::OsStr::new(
                "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin"
            ))
        );
        let terminal = child_path(Some("/Users/ana/bin:/usr/local/bin:/usr/bin".into()));
        assert_eq!(
            terminal.as_deref(),
            Some(std::ffi::OsStr::new(
                "/Users/ana/bin:/usr/local/bin:/usr/bin:/opt/homebrew/bin:/bin:/usr/sbin:/sbin"
            ))
        );
    }

    #[test]
    fn removes_inherited_repository_redirects() {
        let cmd = Git::at("git").write("/repo").build();
        for key in BLOCKED_ENV {
            assert_eq!(env_of(&cmd, key), Some(None), "{key} must be removed");
        }
    }

    #[test]
    fn read_and_write_environments() {
        let read = Git::at("git").read("/repo").build();
        assert_eq!(env_of(&read, "LC_ALL"), Some(Some("C".into())));
        assert_eq!(env_of(&read, "GIT_TERMINAL_PROMPT"), Some(Some("0".into())));
        assert_eq!(env_of(&read, "GIT_OPTIONAL_LOCKS"), Some(Some("0".into())));

        let write = Git::at("git").write("/repo").build();
        assert_eq!(env_of(&write, "LC_ALL"), Some(Some("C".into())));
        assert_eq!(
            env_of(&write, "GIT_TERMINAL_PROMPT"),
            Some(Some("0".into()))
        );
        assert_eq!(env_of(&write, "GIT_OPTIONAL_LOCKS"), None);
    }
}
