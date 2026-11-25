//! `--smoke`: the CI starts the built app with this flag on every system. The
//! window opens, the interface loads in the system's web engine and checks
//! what it needs from it (see `src/app/smoke.ts`) and reports here. Then it
//! quits the way the user does (⌘Q / "Quit" with unsaved conflict choices,
//! confirming the warning), and the app must end by itself.
//!
//! A smoke run never touches the user's GitCRAFT configuration: it uses a
//! new, empty configuration folder of its own (no saved tabs, no recent
//! repositories), removed when the app ends.
//!
//! Exit codes: 0 when the interface was fine and the app closed itself; 1
//! with the problems found; 2 if no report arrives in time; 3 if the app is
//! still running after being asked to quit. Without the flag none of this
//! runs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gitcraft_core::{CancellationToken, Git, Result};
use tauri::{AppHandle, State, WebviewWindow};

use crate::error::IpcError;

/// How long the app has to open its window, load the interface and report.
/// On macOS the time the computer sleeps does not count.
const LIMIT: Duration = Duration::from_secs(120);

/// How long the app has to close after the interface asks it to quit.
const QUIT_LIMIT: Duration = Duration::from_secs(30);

/// Whether this run was asked to be a smoke run.
fn requested() -> bool {
    std::env::args().any(|arg| arg == "--smoke")
}

/// Ends a smoke run that never reports. Called before anything else: Tauri
/// creates the window and its web engine before the app's `setup`, and a
/// hang there must end too.
pub fn start_deadline() {
    if requested() {
        std::thread::spawn(|| {
            std::thread::sleep(LIMIT);
            eprintln!("smoke: the interface did not report within {LIMIT:?}");
            std::process::exit(2);
        });
    }
}

/// Whether this run is a smoke run, and its own configuration folder.
pub struct Smoke {
    enabled: bool,
    config: Option<PathBuf>,
}

impl Smoke {
    pub fn from_args() -> std::io::Result<Self> {
        Self::new(requested())
    }

    /// With `enabled`, creates the run's own empty configuration folder.
    pub fn new(enabled: bool) -> std::io::Result<Self> {
        let config = if enabled {
            Some(private_empty_dir(&std::env::temp_dir())?)
        } else {
            None
        };
        Ok(Self { enabled, config })
    }

    /// Where the settings file goes: the run's own folder in a smoke run,
    /// `user_config` (the user's GitCRAFT configuration) otherwise.
    pub fn settings_file(&self, user_config: &Path) -> PathBuf {
        self.config
            .as_deref()
            .unwrap_or(user_config)
            .join("settings.json")
    }

    /// Called when the app is about to end: the closing worked. The run's
    /// configuration folder goes away.
    pub fn on_exit(&self) {
        if self.enabled {
            println!("smoke: the app closed itself when asked to quit");
        }
        if let Some(dir) = &self.config {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Whether the interface should run its checks and report.
#[tauri::command]
#[specta::specta]
pub fn smoke_mode(smoke: State<'_, Smoke>) -> bool {
    smoke.enabled
}

/// For the layout checks: a small repository in the run's own folder, with
/// everything that takes room (a long branch name, labels, parallel lanes,
/// changes of every kind). `None` outside a smoke run.
#[tauri::command]
#[specta::specta]
pub async fn smoke_sample_repository(
    smoke: State<'_, Smoke>,
) -> std::result::Result<Option<String>, IpcError> {
    let Some(dir) = &smoke.config else {
        return Ok(None);
    };
    let repo = dir.join("sample");
    sample_repository(&repo).await?;
    Ok(Some(repo.to_string_lossy().into_owned()))
}

/// The sample repository, stopped at a merge with a conflict.
#[tauri::command]
#[specta::specta]
pub async fn smoke_conflict(
    smoke: State<'_, Smoke>,
) -> std::result::Result<Option<String>, IpcError> {
    let Some(dir) = &smoke.config else {
        return Ok(None);
    };
    let repo = dir.join("conflict");
    conflict_repository(&repo).await?;
    Ok(Some(repo.to_string_lossy().into_owned()))
}

async fn conflict_repository(repo: &Path) -> Result<()> {
    sample_repository(repo).await?;
    let git = sample_git()?;
    let cancel = CancellationToken::new();
    let run = async |args: &[&str]| git.write(repo).args(args).output(&cancel).await.map(drop);
    std::fs::write(repo.join(DELETED), "two\n")?;
    run(&["add", "."]).await?;
    run(&["commit", "-q", "-m", "Our change"]).await?;
    run(&["switch", "-q", "lane-1"]).await?;
    std::fs::write(repo.join(CONFLICTED), "three\n")?;
    run(&["rm", "-q", DELETED]).await?;
    run(&["commit", "-q", "-a", "-m", "Their change"]).await?;
    run(&["switch", "-q", BRANCH]).await?;
    git.write(repo)
        .args(["merge", "-q", "lane-1"])
        .accept_exit_code(1)
        .output(&cancel)
        .await
        .map(drop)
}

const BRANCH: &str = "feature/a-branch-with-quite-a-long-name";
/// Changed on our side, deleted on theirs: kept or deleted whole.
const DELETED: &str = "deleted-by-the-other-side.txt";
const CONFLICTED: &str = "changed-file-with-a-long-name.txt";

/// git with its own identity and none of the user's configuration.
fn sample_git() -> Result<Git> {
    let no_config = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let name = "Persona de pruebas con un nombre bastante largo";
    let email = "persona.de.pruebas.con.correo.largo@example.invalid";
    Ok(Git::discover()?
        .with_env("GIT_CONFIG_GLOBAL", no_config)
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
        .with_env("GIT_AUTHOR_NAME", name)
        .with_env("GIT_AUTHOR_EMAIL", email)
        .with_env("GIT_COMMITTER_NAME", name)
        .with_env("GIT_COMMITTER_EMAIL", email))
}

async fn sample_repository(repo: &Path) -> Result<()> {
    let git = sample_git()?;
    let cancel = CancellationToken::new();
    let run = async |args: &[&str]| git.write(repo).args(args).output(&cancel).await.map(drop);
    let file = |name: &str, text: &str| std::fs::write(repo.join(name), text);
    std::fs::create_dir_all(repo)?;
    run(&["init", "-q", "-b", BRANCH]).await?;
    file("staged-file-with-a-long-name.txt", "one\n")?;
    file(CONFLICTED, "one\n")?;
    file(DELETED, "one\n")?;
    run(&["add", "."]).await?;
    run(&[
        "commit",
        "-q",
        "-m",
        "First commit, with a subject that is not short",
    ])
    .await?;
    for lane in ["lane-1", "lane-2", "lane-3", "lane-4", "lane-5"] {
        run(&["switch", "-q", "-c", lane, BRANCH]).await?;
        let subject = format!("A parallel commit in {lane}");
        run(&["commit", "-q", "--allow-empty", "-m", &subject]).await?;
    }
    run(&["switch", "-q", BRANCH]).await?;
    run(&[
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "Last commit, with a subject that is not short",
    ])
    .await?;
    run(&["tag", "v1.0.0"]).await?;
    run(&["branch", "another-branch-with-a-long-name"]).await?;
    file("staged-file-with-a-long-name.txt", "two\n")?;
    run(&["add", "."]).await?;
    file(CONFLICTED, "two\n")?;
    file("new-file-with-a-long-name.txt", "one\n")?;
    Ok(())
}

/// Resizes the window (inside size, in logical pixels), to lay the interface
/// out at its narrowest. Ignored outside a smoke run.
#[tauri::command]
#[specta::specta]
pub fn smoke_resize(window: WebviewWindow, smoke: State<'_, Smoke>, width: f64, height: f64) {
    if smoke.enabled
        && let Err(err) = window.set_size(tauri::LogicalSize::new(width, height))
    {
        eprintln!("smoke: could not resize the window: {err}");
    }
}

/// The interface's checks. Any problem ends the run with 1. With none, the
/// interface goes on to quit, and the app has [`QUIT_LIMIT`] to end by
/// itself (else 3). `notes` are measures for the log, whatever the result.
/// Ignored outside a smoke run.
#[tauri::command]
#[specta::specta]
pub fn smoke_report(
    app: AppHandle,
    smoke: State<'_, Smoke>,
    problems: Vec<String>,
    notes: Vec<String>,
) {
    if !smoke.enabled {
        return;
    }
    for note in &notes {
        println!("smoke: note: {note}");
    }
    if problems.is_empty() {
        println!("smoke: the interface loaded and has what it needs; quitting");
        std::thread::spawn(|| {
            std::thread::sleep(QUIT_LIMIT);
            eprintln!("smoke: the app did not close within {QUIT_LIMIT:?} of being asked to quit");
            std::process::exit(3);
        });
    } else {
        for problem in &problems {
            eprintln!("smoke: {problem}");
        }
        app.exit(1);
    }
}

/// A new folder under `parent` only this user can read.
fn private_empty_dir(parent: &Path) -> std::io::Result<PathBuf> {
    let mut n = 0u32;
    loop {
        let dir = parent.join(format!("gitcraft-smoke-{}-{n}", std::process::id()));
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        match builder.create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_smoke_run_never_uses_the_users_configuration() {
        let user = Path::new("/home/someone/.config/gitcraft");
        let smoke = Smoke::new(true).expect("smoke");
        let settings = smoke.settings_file(user);
        assert!(!settings.starts_with(user), "{settings:?}");
        let dir = settings.parent().expect("folder");
        assert!(dir.starts_with(std::env::temp_dir()));
        // New and empty: no saved tabs or recent repositories to restore.
        assert_eq!(std::fs::read_dir(dir).expect("list").count(), 0);
        assert!(!settings.exists());

        // Another run gets another folder.
        let other = Smoke::new(true).expect("smoke");
        assert_ne!(other.settings_file(user), settings);

        smoke.on_exit();
        other.on_exit();
        assert!(!dir.exists(), "removed when the app ends");
    }

    #[tokio::test]
    async fn the_sample_repository_has_what_takes_room() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path().join("sample");
        sample_repository(&repo).await.expect("sample");
        let status = gitcraft_core::ops::status::working_status(
            &Git::discover().expect("git"),
            &repo,
            &CancellationToken::new(),
        )
        .await
        .expect("status");
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.unstaged.len(), 1);
        assert_eq!(status.untracked.len(), 1);
        assert!(status.branch.name.is_some_and(|name| name.len() > 30));

        let repo = dir.path().join("conflict");
        conflict_repository(&repo).await.expect("conflict");
        let status = gitcraft_core::ops::status::working_status(
            &Git::discover().expect("git"),
            &repo,
            &CancellationToken::new(),
        )
        .await
        .expect("status");
        assert_eq!(status.conflicted.len(), 2);
    }

    #[test]
    fn a_normal_run_uses_the_users_configuration() {
        let user = Path::new("/home/someone/.config/gitcraft");
        let normal = Smoke::new(false).expect("normal");
        assert_eq!(normal.settings_file(user), user.join("settings.json"));
        assert!(!normal.enabled);
        normal.on_exit();
    }
}
