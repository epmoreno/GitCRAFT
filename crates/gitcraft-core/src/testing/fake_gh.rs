//! A simulated `gh` for tests, on every system: the `fake-gh` test binary
//! copied into a temporary folder as `gh` (`gh.exe` on Windows), with a
//! scenario next to it that says what to answer to each call. It records
//! every call, so no test needs the network or touches the real gh session.
//!
//! The binary is only built with the `testing` feature; tests get its path
//! from `env!("CARGO_BIN_EXE_fake-gh")`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tempfile::TempDir;

const SCENARIO: &str = "scenario.json";
const CALLS: &str = "calls.log";
/// Answered with exit code 0 and not recorded: checks the copy can run.
const PROBE: &str = "--fake-gh-probe";

/// What the simulated `gh` writes and how it ends.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub stdout: String,
    pub stderr: String,
    pub exit: i32,
    /// Waits this long (after writing) before ending.
    pub sleep_ms: u64,
}

impl Reply {
    /// Ends with exit code 0 and writes nothing.
    pub fn ok() -> Self {
        Self::default()
    }

    /// What `gh api --include` writes for an HTTP answer, ending with `exit`.
    pub fn http(status: &str, body: &str, exit: i32) -> Self {
        Self::ok()
            .stdout(&format!(
                "HTTP/2.0 {status}\nContent-Type: application/json\n\n{body}\n"
            ))
            .exit(exit)
    }

    /// Writes `text` to stdout (a line break is not added).
    pub fn stdout(mut self, text: &str) -> Self {
        self.stdout.push_str(text);
        self
    }

    /// Writes `line` and a line break to stdout.
    pub fn out_line(self, line: &str) -> Self {
        self.stdout(&format!("{line}\n"))
    }

    /// Writes `line` and a line break to stderr.
    pub fn err_line(mut self, line: &str) -> Self {
        self.stderr.push_str(line);
        self.stderr.push('\n');
        self
    }

    pub fn exit(mut self, code: i32) -> Self {
        self.exit = code;
        self
    }

    /// Waits `duration` before ending (for time limits and cancellation).
    pub fn sleep(mut self, duration: Duration) -> Self {
        self.sleep_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        self
    }
}

/// One answer: used when the call's arguments start with `prefix`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub prefix: Vec<String>,
    pub reply: Reply,
}

/// Every answer of a simulated `gh`: the first rule that matches, or
/// `otherwise`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    pub rules: Vec<Rule>,
    pub otherwise: Reply,
}

impl Scenario {
    /// The same reply to every call.
    pub fn always(reply: Reply) -> Self {
        Self {
            rules: Vec::new(),
            otherwise: reply,
        }
    }

    /// `reply` for calls whose arguments start with `prefix`
    /// (e.g. `["auth", "status"]`).
    pub fn when(mut self, prefix: &[&str], reply: Reply) -> Self {
        self.rules.push(Rule {
            prefix: prefix.iter().map(|p| (*p).to_owned()).collect(),
            reply,
        });
        self
    }

    fn reply_to(&self, args: &[String]) -> &Reply {
        self.rules
            .iter()
            .find(|rule| args.starts_with(&rule.prefix))
            .map_or(&self.otherwise, |rule| &rule.reply)
    }
}

/// A simulated `gh` in its own temporary folder.
pub struct FakeGh {
    dir: TempDir,
    exe: PathBuf,
}

impl FakeGh {
    /// Copies the `fake-gh` binary at `binary` (from
    /// `env!("CARGO_BIN_EXE_fake-gh")`) and gives it `scenario`.
    pub fn new(binary: &str, scenario: &Scenario) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let exe = dir
            .path()
            .join(format!("gh{}", std::env::consts::EXE_SUFFIX));
        // A copy, not a hard link: on macOS the running binary's path can
        // come back as another link to the same file, i.e. another test's
        // folder and scenario. While copying, the file is open for writing,
        // and on Linux a process another test forks meanwhile inherits that
        // until it runs its own program: running the copy then fails with
        // "Text file busy" (ETXTBSY). So after copying, wait until it runs.
        std::fs::copy(binary, &exe).expect("copy the fake-gh binary");
        wait_until_it_runs(&exe);
        let json = serde_json::to_string(scenario).expect("scenario as JSON");
        std::fs::write(dir.path().join(SCENARIO), json).expect("write the scenario");
        Self { dir, exe }
    }

    /// The simulated `gh` to run.
    pub fn path(&self) -> &Path {
        &self.exe
    }

    /// The arguments of each call so far, joined by spaces, oldest first.
    pub fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join(CALLS))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

fn wait_until_it_runs(exe: &Path) {
    for _ in 0..100 {
        match std::process::Command::new(exe).arg(PROBE).status() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(Duration::from_millis(20));
            }
            result => {
                assert!(result.is_ok_and(|s| s.success()), "fake-gh does not run");
                return;
            }
        }
    }
    panic!("fake-gh stays busy");
}

/// What the `fake-gh` binary does: records the call, then answers it as its
/// scenario says. Returns the exit code.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == PROBE) {
        return 0;
    }
    let dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_owned))
        .expect("fake-gh: where am I?");

    let mut calls = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(CALLS))
        .expect("fake-gh: open the call log");
    writeln!(calls, "{}", args.join(" ")).expect("fake-gh: record the call");
    drop(calls);

    let scenario: Scenario = std::fs::read_to_string(dir.join(SCENARIO))
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .expect("fake-gh: read the scenario");
    let reply = scenario.reply_to(&args);

    let mut stderr = std::io::stderr();
    stderr
        .write_all(reply.stderr.as_bytes())
        .and_then(|()| stderr.flush())
        .expect("fake-gh: write stderr");
    let mut stdout = std::io::stdout();
    stdout
        .write_all(reply.stdout.as_bytes())
        .and_then(|()| stdout.flush())
        .expect("fake-gh: write stdout");
    if reply.sleep_ms > 0 {
        std::thread::sleep(Duration::from_millis(reply.sleep_ms));
    }
    reply.exit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_matching_rule_answers() {
        let scenario = Scenario::always(Reply::ok().exit(9))
            .when(&["api", "--include", "-X"], Reply::ok().exit(1))
            .when(&["api", "--include"], Reply::ok().exit(2));
        let args = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            scenario
                .reply_to(&args("api --include -X POST user/repos"))
                .exit,
            1
        );
        assert_eq!(scenario.reply_to(&args("api --include repos/a/b")).exit, 2);
        assert_eq!(scenario.reply_to(&args("auth status")).exit, 9);
    }
}
