use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

/// Environment variable that forces a specific git executable.
pub const GIT_PATH_OVERRIDE: &str = "GITCRAFT_GIT";

/// Checked after `PATH`. Apps launched from Finder get a minimal `PATH`, so
/// Homebrew installs would otherwise be missed.
const FALLBACK_LOCATIONS: &[&str] = &[
    "/opt/homebrew/bin/git",
    "/usr/local/bin/git",
    "/usr/bin/git",
];

#[cfg(windows)]
const GIT_EXE: &str = "git.exe";
#[cfg(not(windows))]
const GIT_EXE: &str = "git";

/// Finds the git executable: `$GITCRAFT_GIT`, then `$PATH`, then well-known locations.
pub fn find_git() -> Option<PathBuf> {
    find_git_in(
        std::env::var_os(GIT_PATH_OVERRIDE),
        std::env::var_os("PATH"),
        FALLBACK_LOCATIONS,
    )
}

fn find_git_in(
    override_path: Option<OsString>,
    path_var: Option<OsString>,
    fallbacks: &[&str],
) -> Option<PathBuf> {
    // An explicit override that does not point to an executable is an error,
    // not a hint to silently pick another git.
    if let Some(path) = override_path.filter(|p| !p.is_empty()) {
        let path = PathBuf::from(path);
        return is_executable(&path).then_some(path);
    }
    path_var
        .iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join(GIT_EXE))
        .chain(fallbacks.iter().map(PathBuf::from))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl GitVersion {
    /// Oldest git GitCRAFT supports (covers Apple's bundled git).
    pub const MINIMUM: GitVersion = GitVersion {
        major: 2,
        minor: 30,
        patch: 0,
    };

    /// Parses `git --version` output, e.g. `git version 2.39.5 (Apple Git-154)`
    /// or `git version 2.45.1.windows.1`.
    pub fn parse(output: &str) -> Option<Self> {
        let token = output
            .trim()
            .strip_prefix("git version ")?
            .split_whitespace()
            .next()?;
        let mut parts = token.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    pub fn is_supported(&self) -> bool {
        *self >= Self::MINIMUM
    }
}

impl fmt::Display for GitVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u32, minor: u32, patch: u32) -> GitVersion {
        GitVersion {
            major,
            minor,
            patch,
        }
    }

    #[test]
    fn parses_version_variants() {
        assert_eq!(GitVersion::parse("git version 2.55.0\n"), Some(v(2, 55, 0)));
        assert_eq!(
            GitVersion::parse("git version 2.39.5 (Apple Git-154)"),
            Some(v(2, 39, 5))
        );
        assert_eq!(
            GitVersion::parse("git version 2.45.1.windows.1"),
            Some(v(2, 45, 1))
        );
        assert_eq!(
            GitVersion::parse("git version 2.50.0-rc1"),
            Some(v(2, 50, 0))
        );
        assert_eq!(GitVersion::parse("git version 3.0"), Some(v(3, 0, 0)));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(GitVersion::parse(""), None);
        assert_eq!(GitVersion::parse("hg version 6.0"), None);
        assert_eq!(GitVersion::parse("git version x.y"), None);
    }

    #[test]
    fn checks_minimum_version() {
        assert!(v(2, 30, 0).is_supported());
        assert!(v(2, 55, 0).is_supported());
        assert!(!v(2, 29, 9).is_supported());
        assert_eq!(v(2, 39, 5).to_string(), "2.39.5");
    }

    #[cfg(unix)]
    fn fake_git(dir: &Path, executable: bool) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(GIT_EXE);
        std::fs::write(&path, "#!/bin/sh\n").expect("write fake git");
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("chmod");
        path
    }

    #[cfg(unix)]
    #[test]
    fn finds_git_on_path_before_fallbacks() {
        let empty = tempfile::tempdir().expect("tempdir");
        let bin = tempfile::tempdir().expect("tempdir");
        let git = fake_git(bin.path(), true);
        let path_var = std::env::join_paths([empty.path(), bin.path()]).expect("join");
        assert_eq!(
            find_git_in(None, Some(path_var), &["/nonexistent/git"]),
            Some(git)
        );
    }

    #[cfg(unix)]
    #[test]
    fn skips_non_executable_files_and_uses_fallback() {
        let bin = tempfile::tempdir().expect("tempdir");
        fake_git(bin.path(), false);
        let fallback_dir = tempfile::tempdir().expect("tempdir");
        let fallback = fake_git(fallback_dir.path(), true);
        let fallback_str = fallback.to_str().expect("utf8 path");
        assert_eq!(
            find_git_in(None, Some(bin.path().into()), &[fallback_str]),
            Some(fallback.clone())
        );
    }

    #[cfg(unix)]
    #[test]
    fn explicit_override_wins_or_fails() {
        let bin = tempfile::tempdir().expect("tempdir");
        let git = fake_git(bin.path(), true);
        assert_eq!(
            find_git_in(Some(git.clone().into()), None, &[]),
            Some(git.clone())
        );
        assert_eq!(
            find_git_in(
                Some("/nonexistent/git".into()),
                Some(bin.path().into()),
                &[]
            ),
            None
        );
    }
}
