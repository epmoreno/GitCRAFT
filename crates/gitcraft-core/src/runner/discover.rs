use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use crate::platform::{Os, is_executable};

/// Environment variable that forces a specific git executable.
pub const GIT_PATH_OVERRIDE: &str = "GITCRAFT_GIT";

/// Finds the git executable: `$GITCRAFT_GIT`, then `$PATH`, then where each
/// system usually installs it ([`Os::git_locations`]).
pub fn find_git() -> Option<PathBuf> {
    let os = Os::current();
    find_git_in(
        std::env::var_os(GIT_PATH_OVERRIDE),
        std::env::var_os("PATH"),
        &os.exe("git"),
        &os.git_locations(&|var| std::env::var_os(var)),
    )
}

fn find_git_in(
    override_path: Option<OsString>,
    path_var: Option<OsString>,
    exe: &str,
    fallbacks: &[PathBuf],
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
        .map(|dir| dir.join(exe))
        .chain(fallbacks.iter().cloned())
        .find(|candidate| is_executable(candidate))
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

    fn exe() -> String {
        Os::current().exe("git")
    }

    fn fake_git(dir: &std::path::Path, executable: bool) -> PathBuf {
        let path = dir.join(exe());
        std::fs::write(&path, "#!/bin/sh\n").expect("write fake git");
        let mode = if executable { 0o755 } else { 0o644 };
        crate::platform::set_file_mode(&path, mode).expect("chmod");
        path
    }

    #[test]
    fn finds_git_on_path_before_fallbacks() {
        let empty = tempfile::tempdir().expect("tempdir");
        let bin = tempfile::tempdir().expect("tempdir");
        let git = fake_git(bin.path(), true);
        let path_var = std::env::join_paths([empty.path(), bin.path()]).expect("join");
        assert_eq!(
            find_git_in(
                None,
                Some(path_var),
                &exe(),
                &[PathBuf::from("/nonexistent/git")]
            ),
            Some(git)
        );
    }

    #[test]
    fn uses_the_fallbacks_when_path_has_no_git() {
        let empty = tempfile::tempdir().expect("tempdir");
        let fallback_dir = tempfile::tempdir().expect("tempdir");
        let fallback = fake_git(fallback_dir.path(), true);
        assert_eq!(
            find_git_in(
                None,
                Some(empty.path().into()),
                &exe(),
                std::slice::from_ref(&fallback)
            ),
            Some(fallback)
        );
    }

    /// Only Unix has an executable bit to check.
    #[cfg(unix)]
    #[test]
    fn skips_non_executable_files_and_uses_fallback() {
        let bin = tempfile::tempdir().expect("tempdir");
        fake_git(bin.path(), false);
        let fallback_dir = tempfile::tempdir().expect("tempdir");
        let fallback = fake_git(fallback_dir.path(), true);
        assert_eq!(
            find_git_in(
                None,
                Some(bin.path().into()),
                &exe(),
                std::slice::from_ref(&fallback)
            ),
            Some(fallback)
        );
    }

    #[test]
    fn explicit_override_wins_or_fails() {
        let bin = tempfile::tempdir().expect("tempdir");
        let git = fake_git(bin.path(), true);
        assert_eq!(
            find_git_in(Some(git.clone().into()), None, &exe(), &[]),
            Some(git.clone())
        );
        assert_eq!(
            find_git_in(
                Some("/nonexistent/git".into()),
                Some(bin.path().into()),
                &exe(),
                &[]
            ),
            None
        );
    }
}
