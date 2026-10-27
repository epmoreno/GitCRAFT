use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::{CancellationToken, Git, GitError, Result};

/// Opening only reads a couple of paths, but network volumes can hang.
const OPEN_TIMEOUT: Duration = Duration::from_secs(20);

/// A repository with a working tree, identified by its top-level directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RepoInfo {
    /// Absolute path of the working tree root, as git reports it.
    pub path: String,
    /// Directory name, used as the tab title.
    pub name: String,
    /// Absolute path of the `.git` directory.
    pub git_dir: String,
    /// Absolute path of the directory shared by all worktrees (refs, objects).
    /// Equal to `git_dir` except in linked worktrees.
    pub common_dir: String,
}

/// Validates that `path` is (inside) a non-bare repository and resolves its root.
///
/// Opening a subdirectory opens the repository that contains it.
pub async fn open_repository(
    git: &Git,
    path: &Path,
    cancel: &CancellationToken,
) -> Result<RepoInfo> {
    if !path.is_dir() {
        return Err(GitError::PathNotFound(path.to_path_buf()));
    }
    let out = git
        .read(path)
        .args([
            "rev-parse",
            "--show-toplevel",
            "--absolute-git-dir",
            "--git-common-dir",
        ])
        .timeout(OPEN_TIMEOUT)
        .output(cancel)
        .await?;
    parse_open_output(&out.stdout_lossy(), path)
}

/// Parses the lines printed by `rev-parse --show-toplevel --absolute-git-dir
/// --git-common-dir`. The common dir may be relative to `cwd`.
fn parse_open_output(stdout: &str, cwd: &Path) -> Result<RepoInfo> {
    let mut lines = stdout.lines();
    let (Some(toplevel), Some(git_dir), Some(common_dir)) =
        (lines.next(), lines.next(), lines.next())
    else {
        return Err(GitError::Parse(format!(
            "rev-parse --show-toplevel --absolute-git-dir --git-common-dir: {stdout:?}"
        )));
    };
    let common_dir = if Path::new(common_dir).is_absolute() {
        PathBuf::from(common_dir)
    } else {
        let joined = cwd.join(common_dir);
        joined.canonicalize().unwrap_or(joined)
    };
    let name = Path::new(toplevel)
        .file_name()
        .map_or_else(|| toplevel.to_owned(), |n| n.to_string_lossy().into_owned());
    Ok(RepoInfo {
        path: toplevel.to_owned(),
        name,
        git_dir: git_dir.to_owned(),
        common_dir: common_dir.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_toplevel_git_dir_and_common_dir() {
        let info = parse_open_output(
            "/Users/ana/src/my app\n/Users/ana/src/my app/.git\n/Users/ana/src/my app/.git\n",
            Path::new("/Users/ana/src/my app"),
        )
        .expect("parse");
        assert_eq!(
            info,
            RepoInfo {
                path: "/Users/ana/src/my app".into(),
                name: "my app".into(),
                git_dir: "/Users/ana/src/my app/.git".into(),
                common_dir: "/Users/ana/src/my app/.git".into(),
            }
        );
    }

    #[test]
    fn resolves_a_relative_common_dir_against_the_working_directory() {
        let info =
            parse_open_output("/r\n/r/.git\n.git\n", Path::new("/does/not/exist")).expect("parse");
        assert_eq!(info.common_dir, "/does/not/exist/.git");
    }

    #[test]
    fn uses_the_path_as_name_for_the_filesystem_root() {
        let info = parse_open_output("/\n/.git\n/.git\n", Path::new("/")).expect("parse");
        assert_eq!(info.name, "/");
    }

    #[test]
    fn rejects_truncated_output() {
        assert!(matches!(
            parse_open_output("/only\n/only/.git\n", Path::new("/")),
            Err(GitError::Parse(_))
        ));
        assert!(matches!(
            parse_open_output("", Path::new("/")),
            Err(GitError::Parse(_))
        ));
    }
}
