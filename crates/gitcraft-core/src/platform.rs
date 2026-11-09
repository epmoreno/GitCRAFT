//! File handling that differs between operating systems, in one place (the
//! inventory is in `docs/ARCHITECTURE.md`, section 8).

use std::ffi::OsString;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

/// The operating systems GitCRAFT knows. Functions that differ take one, so
/// tests can check every system's rules on any machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

impl Os {
    /// The system this build runs on (other Unix systems follow Linux).
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// `name` as an executable file name (`git.exe` on Windows).
    pub fn exe(self, name: &str) -> String {
        match self {
            Self::Windows => format!("{name}.exe"),
            Self::MacOs | Self::Linux => name.to_owned(),
        }
    }

    /// Where git may be installed when it is not on `PATH` (apps started
    /// from Finder get a minimal `PATH`). `env` reads environment variables.
    pub fn git_locations(self, env: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
        match self {
            Self::MacOs => paths(&[
                "/opt/homebrew/bin/git",
                "/usr/local/bin/git",
                "/usr/bin/git",
            ]),
            Self::Linux => paths(&["/usr/bin/git", "/usr/local/bin/git"]),
            Self::Windows => windows_paths(
                env,
                &[
                    ("ProgramFiles", r"Git\cmd\git.exe"),
                    ("ProgramW6432", r"Git\cmd\git.exe"),
                    ("ProgramFiles(x86)", r"Git\cmd\git.exe"),
                    ("LOCALAPPDATA", r"Programs\Git\cmd\git.exe"),
                ],
            ),
        }
    }

    /// Where GitHub CLI may be installed when it is not on `PATH`.
    pub fn gh_locations(self, env: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
        match self {
            Self::MacOs => paths(&["/opt/homebrew/bin/gh", "/usr/local/bin/gh"]),
            Self::Linux => paths(&[
                "/usr/bin/gh",
                "/usr/local/bin/gh",
                "/snap/bin/gh",
                "/home/linuxbrew/.linuxbrew/bin/gh",
            ]),
            Self::Windows => windows_paths(
                env,
                &[
                    ("ProgramFiles", r"GitHub CLI\gh.exe"),
                    ("ProgramW6432", r"GitHub CLI\gh.exe"),
                    ("ProgramFiles(x86)", r"GitHub CLI\gh.exe"),
                    ("LOCALAPPDATA", r"Programs\GitHub CLI\gh.exe"),
                ],
            ),
        }
    }

    /// Folders added (when missing) to the `PATH` of git and gh, so the
    /// helpers they start (credential helpers, editors) are found even with
    /// the minimal `PATH` of an app started from Finder. Windows apps get the
    /// user's full `PATH`: nothing to add.
    pub fn extra_path_dirs(self) -> &'static [&'static str] {
        match self {
            Self::MacOs => &[
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
                "/usr/sbin",
                "/sbin",
            ],
            Self::Linux => &["/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"],
            Self::Windows => &[],
        }
    }

    /// The program and arguments that open `url` in the default browser,
    /// without a shell.
    pub fn open_url_command(self, url: &str) -> (&'static str, Vec<String>) {
        match self {
            Self::MacOs => ("/usr/bin/open", vec![url.to_owned()]),
            Self::Linux => ("xdg-open", vec![url.to_owned()]),
            Self::Windows => ("explorer.exe", vec![url.to_owned()]),
        }
    }
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

/// `<value of the variable>\<rest>` for each variable that is set, without
/// repeats (`ProgramFiles` and `ProgramW6432` are usually the same).
fn windows_paths(env: &dyn Fn(&str) -> Option<OsString>, list: &[(&str, &str)]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for (var, rest) in list {
        if let Some(base) = env(var).filter(|b| !b.is_empty()) {
            let path = PathBuf::from(format!(
                "{}\\{rest}",
                base.to_string_lossy().trim_end_matches('\\')
            ));
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

/// Whether `path` is a file this system can run: the executable bit on Unix,
/// any file elsewhere.
pub fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// What this system can put back exactly when undoing a discard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSupport {
    /// Symbolic links can be created again.
    pub symlinks: bool,
}

impl FileSupport {
    /// This system: Unix creates symbolic links; on Windows that is not
    /// done yet (P.8).
    pub const fn current() -> Self {
        Self {
            symlinks: cfg!(unix),
        }
    }
}

/// A file's mode bits (permissions, including the executable bit). Systems
/// without them report a regular file's mode, as Git for Windows does.
pub fn file_mode(meta: &Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        0o644
    }
}

/// Applies mode bits to a file. Does nothing on systems without them: Git
/// for Windows does not apply the executable bit either (it stays recorded
/// in the index).
pub fn set_file_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// Creates a symbolic link at `path` pointing to `target`. Fails where
/// [`FileSupport::current`] says symbolic links are not supported.
pub fn create_symlink(target: &Path, path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, path)
    }
    #[cfg(not(unix))]
    {
        let _ = (target, path);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "symbolic links cannot be created on this system yet",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows_env(var: &str) -> Option<OsString> {
        match var {
            "ProgramFiles" | "ProgramW6432" => Some(r"C:\Program Files".into()),
            "ProgramFiles(x86)" => Some(r"C:\Program Files (x86)".into()),
            "LOCALAPPDATA" => Some(r"C:\Users\ana\AppData\Local\".into()),
            _ => None,
        }
    }

    fn strings(paths: Vec<PathBuf>) -> Vec<String> {
        paths
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    /// P.6: where each system looks for git and gh, with simulated
    /// environments.
    #[test]
    fn finds_git_and_gh_where_each_system_installs_them() {
        let no_env = |_: &str| None;
        assert_eq!(
            strings(Os::MacOs.git_locations(&no_env)),
            [
                "/opt/homebrew/bin/git",
                "/usr/local/bin/git",
                "/usr/bin/git"
            ]
        );
        assert_eq!(
            strings(Os::MacOs.gh_locations(&no_env)),
            ["/opt/homebrew/bin/gh", "/usr/local/bin/gh"]
        );
        assert_eq!(
            strings(Os::Linux.git_locations(&no_env)),
            ["/usr/bin/git", "/usr/local/bin/git"]
        );
        assert!(
            Os::Linux
                .gh_locations(&no_env)
                .iter()
                .all(|p| !p.starts_with("/opt/homebrew"))
        );
        assert_eq!(
            strings(Os::Windows.git_locations(&windows_env)),
            [
                r"C:\Program Files\Git\cmd\git.exe",
                r"C:\Program Files (x86)\Git\cmd\git.exe",
                r"C:\Users\ana\AppData\Local\Programs\Git\cmd\git.exe",
            ]
        );
        assert_eq!(
            strings(Os::Windows.gh_locations(&windows_env)),
            [
                r"C:\Program Files\GitHub CLI\gh.exe",
                r"C:\Program Files (x86)\GitHub CLI\gh.exe",
                r"C:\Users\ana\AppData\Local\Programs\GitHub CLI\gh.exe",
            ]
        );
        assert!(Os::Windows.git_locations(&no_env).is_empty());
    }

    #[test]
    fn names_executables_and_opens_urls_per_system() {
        assert_eq!(Os::Windows.exe("gh"), "gh.exe");
        assert_eq!(Os::Linux.exe("gh"), "gh");
        let url = "https://github.com/login/device";
        assert_eq!(
            Os::MacOs.open_url_command(url),
            ("/usr/bin/open", vec![url.to_owned()])
        );
        assert_eq!(Os::Linux.open_url_command(url).0, "xdg-open");
        assert_eq!(Os::Windows.open_url_command(url).0, "explorer.exe");
    }
}
