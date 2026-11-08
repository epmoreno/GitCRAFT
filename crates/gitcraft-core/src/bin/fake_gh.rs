//! A simulated `gh` for the tests (see `gitcraft_core::testing::fake_gh`).
//! Only built with the `testing` feature; never part of the app.

fn main() {
    std::process::exit(gitcraft_core::testing::fake_gh::run());
}
