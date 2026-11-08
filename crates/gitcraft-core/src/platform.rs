//! File handling that differs between operating systems, in one place (the
//! inventory is in `docs/ARCHITECTURE.md`, section 8).

use std::fs::Metadata;
use std::io;
use std::path::Path;

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
