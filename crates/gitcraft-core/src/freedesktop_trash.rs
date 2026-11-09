//! The freedesktop.org Trash, as Linux file managers use it:
//! <https://specifications.freedesktop.org/trash-spec/1.0/>.
//!
//! A trashed file moves to `<trash>/files/<name>` and gets a
//! `<trash>/info/<name>.trashinfo` with its original path and the deletion
//! date. Files on the home volume go to the home Trash
//! (`$XDG_DATA_HOME/Trash`); files on another volume go to that volume's
//! Trash (`$topdir/.Trash/$uid` or `$topdir/.Trash-$uid`). When no Trash is
//! available the file is left where it is: nothing is ever deleted instead.
//!
//! Built on every Unix so its tests also run on macOS (with a temporary home
//! Trash); `SystemTrash` uses it outside macOS.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// A freedesktop Trash: the home one, plus one per other volume.
#[derive(Debug, Clone)]
pub struct FreedesktopTrash {
    /// `$XDG_DATA_HOME/Trash`.
    home: PathBuf,
    uid: u32,
}

impl FreedesktopTrash {
    /// The current user's Trash (`$XDG_DATA_HOME/Trash`, by default
    /// `~/.local/share/Trash`).
    pub fn for_current_user() -> io::Result<Self> {
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .map(|home| home.join(".local/share"))
            })
            .ok_or_else(|| io::Error::other("neither XDG_DATA_HOME nor HOME is set"))?;
        Ok(Self::with_home_trash(
            data_home.join("Trash"),
            nix::unistd::getuid().as_raw(),
        ))
    }

    /// A Trash whose home Trash is `home` (tests use a temporary one).
    pub fn with_home_trash(home: PathBuf, uid: u32) -> Self {
        Self { home, uid }
    }

    /// Moves `path` (a file, symlink or directory) to the Trash and returns
    /// where it is now. On error the file has not moved.
    pub fn trash(&self, path: &Path) -> io::Result<PathBuf> {
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other(format!("cannot trash {}", path.display())))?;
        let (trash, topdir) = self.trash_for(path)?;
        let files = trash.join("files");
        let info = trash.join("info");
        create_private_dir(&files)?;
        create_private_dir(&info)?;

        let original = match &topdir {
            Some(top) => path.strip_prefix(top).unwrap_or(path).to_path_buf(),
            None => path.to_path_buf(),
        };
        let contents = format!(
            "[Trash Info]\nPath={}\nDeletionDate={}\n",
            percent_encode(&original),
            local_now()
        );

        for n in 1u32.. {
            let mut candidate = name.to_os_string();
            if n > 1 {
                candidate.push(format!(".{n}"));
            }
            let mut info_name = candidate.clone();
            info_name.push(".trashinfo");
            let info_path = info.join(&info_name);
            let target = files.join(&candidate);
            // Creating the info file atomically reserves the name.
            let mut file = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&info_path)
            {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            };
            if fs::symlink_metadata(&target).is_ok() {
                // A file without its info: leave both alone.
                drop(file);
                fs::remove_file(&info_path)?;
                continue;
            }
            let moved = file
                .write_all(contents.as_bytes())
                .and_then(|()| file.sync_all())
                .and_then(|()| fs::rename(path, &target));
            if let Err(e) = moved {
                let _ = fs::remove_file(&info_path);
                return Err(e);
            }
            return Ok(target);
        }
        unreachable!("u32 names run out")
    }

    /// The Trash for `path`, and the volume's top directory when it is not
    /// the home Trash (its info files hold paths relative to it).
    fn trash_for(&self, path: &Path) -> io::Result<(PathBuf, Option<PathBuf>)> {
        let device = fs::symlink_metadata(path)?.dev();
        if create_private_dir(&self.home).is_ok()
            && fs::metadata(&self.home).is_ok_and(|m| m.dev() == device)
        {
            return Ok((self.home.clone(), None));
        }
        let top = top_dir(path, device)?;

        // $topdir/.Trash: set up by an administrator, sticky and not a link.
        let shared = top.join(".Trash");
        if fs::symlink_metadata(&shared)
            .is_ok_and(|m| m.is_dir() && m.permissions().mode() & 0o1000 != 0)
        {
            let mine = shared.join(self.uid.to_string());
            if create_private_dir(&mine).is_ok() && is_own_dir(&mine, self.uid) {
                return Ok((mine, Some(top)));
            }
        }
        // $topdir/.Trash-$uid: the user's own.
        let own = top.join(format!(".Trash-{}", self.uid));
        if create_private_dir(&own).is_ok() && is_own_dir(&own, self.uid) {
            return Ok((own, Some(top)));
        }
        Err(io::Error::other(format!(
            "there is no Trash on the volume of {}",
            path.display()
        )))
    }
}

/// After undoing: `trashed_to` is back in place, so its info file goes.
pub fn forget(trashed_to: &Path) -> io::Result<()> {
    let (Some(files), Some(name)) = (trashed_to.parent(), trashed_to.file_name()) else {
        return Ok(());
    };
    if files.file_name().is_none_or(|n| n != "files") {
        return Ok(());
    }
    let Some(trash) = files.parent() else {
        return Ok(());
    };
    let mut info_name = name.to_os_string();
    info_name.push(".trashinfo");
    match fs::remove_file(trash.join("info").join(info_name)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

fn is_own_dir(dir: &Path, uid: u32) -> bool {
    fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && m.uid() == uid)
}

/// The mount point that holds `path` (on `device`).
fn top_dir(path: &Path, device: u64) -> io::Result<PathBuf> {
    let mut top = path.to_path_buf();
    while let Some(parent) = top.parent() {
        if fs::metadata(parent)?.dev() != device {
            break;
        }
        top = parent.to_path_buf();
    }
    Ok(top)
}

/// The path as the spec writes it: bytes outside the unreserved set and
/// `/` are percent-encoded.
fn percent_encode(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut out = String::new();
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Local time as `YYYY-MM-DDThh:mm:ss`, as the spec asks.
#[allow(unsafe_code)] // localtime_r: the only way to the local time in std + libc.
fn local_now() -> String {
    use nix::libc;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let seconds = libc::time_t::try_from(now).unwrap_or(0);
    // SAFETY: `tm` is plain data, and localtime_r only writes into it.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are valid for the duration of the call.
    if unsafe { libc::localtime_r(&seconds, &mut tm) }.is_null() {
        return "1970-01-01T00:00:00".to_owned();
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_paths_as_the_spec_asks() {
        assert_eq!(
            percent_encode(Path::new("/home/ana/año 1/a&b.txt")),
            "/home/ana/a%C3%B1o%201/a%26b.txt"
        );
    }

    #[test]
    fn writes_the_date_in_the_spec_format() {
        let date = local_now();
        assert_eq!(date.len(), 19, "{date}");
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[10..11], "T");
    }
}
