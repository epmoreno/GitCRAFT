//! Cloning a repository into a new folder.
//!
//! The destination must not exist yet, or be an empty folder: GitCRAFT
//! never clones into (or over) a folder with files. Authentication is the
//! credential helper's or SSH agent's, as for fetch.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{CancellationToken, Git, GitError, Result};

/// Whether a clone could go to a destination, checked as it is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Destination {
    /// Nothing there: the folder is created.
    Free,
    /// An empty folder: the clone goes inside.
    EmptyFolder,
    /// Something is there (a non-empty folder, or a file): refused.
    NotEmpty,
    /// The parent folder does not exist.
    ParentMissing,
    /// Not usable as a folder name (empty, ".", "..", contains "/").
    InvalidName,
}

/// The folder name a clone of `url` gets by default: the last path
/// component, without a trailing ".git".
pub fn default_folder_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches(['/', '\\']);
    let last = trimmed.rsplit(['/', '\\', ':']).next().unwrap_or(trimmed);
    let name = last.strip_suffix(".git").unwrap_or(last);
    if name.is_empty() || name == "." || name == ".." {
        String::new()
    } else {
        name.to_owned()
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

/// What cloning into `parent/name` would find there.
pub fn check_destination(parent: &Path, name: &str) -> Destination {
    if !valid_name(name) {
        return Destination::InvalidName;
    }
    if !parent.is_dir() {
        return Destination::ParentMissing;
    }
    let path = parent.join(name);
    match std::fs::symlink_metadata(&path) {
        Err(_) => Destination::Free,
        Ok(meta) if meta.is_dir() => {
            let empty = std::fs::read_dir(&path).is_ok_and(|mut entries| entries.next().is_none());
            if empty {
                Destination::EmptyFolder
            } else {
                Destination::NotEmpty
            }
        }
        Ok(_) => Destination::NotEmpty,
    }
}

/// Clones `url` into `parent/name`, reporting git's progress. Returns the
/// new repository's path. Refuses a destination that is not free or an
/// empty folder, and URLs that could be read as options.
pub async fn clone_repository(
    git: &Git,
    url: &str,
    parent: &Path,
    name: &str,
    cancel: &CancellationToken,
    on_progress: impl FnMut(&str),
) -> Result<PathBuf> {
    let url = url.trim();
    if url.is_empty() || url.starts_with('-') || url.contains('\0') {
        return Err(GitError::Parse(format!("invalid clone URL {url:?}")));
    }
    match check_destination(parent, name) {
        Destination::Free | Destination::EmptyFolder => {}
        Destination::ParentMissing => return Err(GitError::PathNotFound(parent.to_path_buf())),
        Destination::InvalidName => {
            return Err(GitError::Parse(format!("invalid folder name {name:?}")));
        }
        Destination::NotEmpty => return Err(GitError::DestinationNotEmpty(parent.join(name))),
    }
    let destination = parent.join(name);
    git.write(parent)
        .args(["clone", "--progress"])
        .env("GIT_PROGRESS_DELAY", "0")
        .arg("--")
        .arg(url)
        .arg(&destination)
        .output_with_progress(cancel, on_progress)
        .await?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_the_folder_name_from_the_url() {
        for (url, name) in [
            ("https://example.com/team/project.git", "project"),
            ("https://example.com/team/project", "project"),
            ("https://example.com/team/project/", "project"),
            ("git@example.com:team/tool.git", "tool"),
            ("example.com:tool", "tool"),
            ("file:///Users/me/gitcraft-remoto.git", "gitcraft-remoto"),
            ("/Users/me/repos/local", "local"),
            ("  https://example.com/x.git  ", "x"),
            ("https://example.com/", "example.com"),
            ("", ""),
            (".git", ""),
        ] {
            assert_eq!(default_folder_name(url), name, "{url:?}");
        }
    }
}
