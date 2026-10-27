//! GitCRAFT core: everything that talks to `git` and interprets its output.
//!
//! This crate has no Tauri dependency so it can be tested in isolation.

pub mod error;
pub mod runner;
#[cfg(feature = "testing")]
pub mod testing;

pub use error::{GitError, Result};
pub use runner::{Access, Git, GitCommand, GitOutput, GitVersion};
pub use tokio_util::sync::CancellationToken;
