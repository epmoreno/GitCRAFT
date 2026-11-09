use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use crate::platform::native_path;
use crate::{CancellationToken, Git, GitError, Result};

/// Opening only reads a couple of paths, but network volumes can hang.
const OPEN_TIMEOUT: Duration = Duration::from_secs(20);

/// A repository with a working tree, identified by its top-level directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RepoInfo {
    /// Absolute path of the working tree root, as git reports it (in the
    /// system's own form: [`native_path`]).
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
        native_path(common_dir)
    } else {
        resolve(&cwd.join(common_dir))
    };
    let toplevel = native_path(toplevel);
    let name = Path::new(&toplevel)
        .file_name()
        .map_or_else(|| toplevel.clone(), |n| n.to_string_lossy().into_owned());
    Ok(RepoInfo {
        name,
        path: toplevel,
        git_dir: native_path(git_dir),
        common_dir,
    })
}

/// A path relative to the working directory, resolved where it exists.
fn resolve(joined: &Path) -> String {
    #[cfg(windows)]
    {
        native_path(&joined.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        joined
            .canonicalize()
            .unwrap_or_else(|_| joined.to_path_buf())
            .to_string_lossy()
            .into_owned()
    }
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
        // One separator only: the system's.
        let expected = if cfg!(windows) {
            r"\does\not\exist\.git"
        } else {
            "/does/not/exist/.git"
        };
        assert_eq!(info.common_dir, expected);
    }

    #[test]
    fn strips_the_verbatim_prefix() {
        use crate::platform::without_verbatim_prefix;
        assert_eq!(
            without_verbatim_prefix(r"\\?\C:\Users\ana"),
            r"C:\Users\ana"
        );
        assert_eq!(
            without_verbatim_prefix(r"\\?\UNC\server\share\repo"),
            r"\\server\share\repo"
        );
        assert_eq!(without_verbatim_prefix("/Users/ana"), "/Users/ana");
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
