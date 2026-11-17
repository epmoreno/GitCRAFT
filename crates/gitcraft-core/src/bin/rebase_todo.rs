//! The interactive rebase's todo helper as a program of its own, for the
//! tests (`gitcraft_core::ops::interactive_rebase::todo_helper_main`; the
//! app runs the same code from its own executable). Only built with the
//! `testing` feature; never part of the app.

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let code = gitcraft_core::ops::interactive_rebase::todo_helper_main(&args).unwrap_or(2);
    std::process::exit(code);
}
