// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // git's sequence editor for GitCRAFT's interactive rebase: no window.
    if let Some(code) = gitcraft_lib::rebase_todo_helper() {
        std::process::exit(code);
    }
    gitcraft_lib::run()
}
