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

use tauri::{AppHandle, State};

/// How long the interface has to load and report.
const LIMIT: Duration = Duration::from_secs(120);

/// How long the app has to close after the interface asks it to quit.
const QUIT_LIMIT: Duration = Duration::from_secs(30);

/// Whether this run is a smoke run, and its own configuration folder.
pub struct Smoke {
    enabled: bool,
    config: Option<PathBuf>,
}

impl Smoke {
    pub fn from_args() -> std::io::Result<Self> {
        Self::new(std::env::args().any(|arg| arg == "--smoke"))
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

    /// Ends a smoke run that never reports.
    pub fn start_deadline(&self) {
        if self.enabled {
            std::thread::spawn(|| {
                std::thread::sleep(LIMIT);
                eprintln!("smoke: the interface did not report within {LIMIT:?}");
                std::process::exit(2);
            });
        }
    }
}

/// Whether the interface should run its checks and report.
#[tauri::command]
#[specta::specta]
pub fn smoke_mode(smoke: State<'_, Smoke>) -> bool {
    smoke.enabled
}

/// The interface's checks. Any problem ends the run with 1. With none, the
/// interface goes on to quit, and the app has [`QUIT_LIMIT`] to end by
/// itself (else 3). Ignored outside a smoke run.
#[tauri::command]
#[specta::specta]
pub fn smoke_report(app: AppHandle, smoke: State<'_, Smoke>, problems: Vec<String>) {
    if !smoke.enabled {
        return;
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

    #[test]
    fn a_normal_run_uses_the_users_configuration() {
        let user = Path::new("/home/someone/.config/gitcraft");
        let normal = Smoke::new(false).expect("normal");
        assert_eq!(normal.settings_file(user), user.join("settings.json"));
        assert!(!normal.enabled);
        normal.on_exit();
    }
}
