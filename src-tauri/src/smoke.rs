//! `--smoke`: the CI starts the built app with this flag on every system. The
//! window opens, the interface loads in the system's web engine and checks
//! what it needs from it (see `src/app/smoke.ts`) and reports here. Then it
//! quits the way the user does (⌘Q / "Quit" with unsaved conflict choices,
//! confirming the warning), and the app must end by itself.
//!
//! Exit codes: 0 when the interface was fine and the app closed itself; 1
//! with the problems found; 2 if no report arrives in time; 3 if the app is
//! still running after being asked to quit. Without the flag none of this
//! runs.

use std::time::Duration;

use tauri::{AppHandle, State};

/// How long the interface has to load and report.
const LIMIT: Duration = Duration::from_secs(120);

/// How long the app has to close after the interface asks it to quit.
const QUIT_LIMIT: Duration = Duration::from_secs(30);

/// Whether this run is a smoke run.
pub struct Smoke(pub bool);

impl Smoke {
    pub fn from_args() -> Self {
        Self(std::env::args().any(|arg| arg == "--smoke"))
    }

    /// Called when the app is about to end: the closing worked.
    pub fn on_exit(&self) {
        if self.0 {
            println!("smoke: the app closed itself when asked to quit");
        }
    }

    /// Ends a smoke run that never reports.
    pub fn start_deadline(&self) {
        if self.0 {
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
    smoke.0
}

/// The interface's checks. Any problem ends the run with 1. With none, the
/// interface goes on to quit, and the app has [`QUIT_LIMIT`] to end by
/// itself (else 3). Ignored outside a smoke run.
#[tauri::command]
#[specta::specta]
pub fn smoke_report(app: AppHandle, smoke: State<'_, Smoke>, problems: Vec<String>) {
    if !smoke.0 {
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
