//! GitCRAFT core: everything that talks to `git` and interprets its output.
//!
//! This crate has no Tauri dependency so it can be tested in isolation.

pub mod error;
#[cfg(unix)]
pub mod freedesktop_trash;
pub mod gh;
pub mod graph;
pub mod ops;
pub mod parse;
pub mod patch;
pub mod platform;
pub mod redact;
pub mod runner;
#[cfg(feature = "testing")]
pub mod testing;

pub use error::{ErrorKind, GitError, Result};
pub use runner::{
    Access, CommandEvent, CommandLog, CommandOutcome, Git, GitCommand, GitOutput, GitVersion,
};
pub use tokio_util::sync::CancellationToken;
