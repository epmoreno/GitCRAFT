//! GitCRAFT core: everything that talks to `git` and interprets its output.
//!
//! This crate has no Tauri dependency so it can be tested in isolation.

pub mod error;
pub mod graph;
pub mod ops;
pub mod parse;
pub mod patch;
pub mod runner;
#[cfg(feature = "testing")]
pub mod testing;

pub use error::{ErrorKind, GitError, Result};
pub use runner::{Access, Git, GitCommand, GitOutput, GitVersion};
pub use tokio_util::sync::CancellationToken;
