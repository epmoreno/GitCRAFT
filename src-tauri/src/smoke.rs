//! `--smoke`: the CI starts the built app with this flag on every system. The
//! window opens, the interface loads in the system's web engine and checks
//! what it needs from it (see `src/app/smoke.ts`), then reports here and the
//! app exits: 0 when everything is there, 1 with the problems otherwise, 2
//! if no report arrives in time. Without the flag none of this runs.

use std::time::Duration;

use tauri::{AppHandle, State};

/// How long the interface has to load and report.
const LIMIT: Duration = Duration::from_secs(120);

/// Whether this run is a smoke run.
pub struct Smoke(pub bool);

impl Smoke {
    pub fn from_args() -> Self {
        Self(std::env::args().any(|arg| arg == "--smoke"))
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

/// The interface's checks: no problems ends the run with 0, any with 1.
/// Ignored outside a smoke run.
#[tauri::command]
#[specta::specta]
pub fn smoke_report(app: AppHandle, smoke: State<'_, Smoke>, problems: Vec<String>) {
    if !smoke.0 {
        return;
    }
    if problems.is_empty() {
        println!("smoke: the interface loaded and has what it needs");
        app.exit(0);
    } else {
        for problem in &problems {
            eprintln!("smoke: {problem}");
        }
        app.exit(1);
    }
}
