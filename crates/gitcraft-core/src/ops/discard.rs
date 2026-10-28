//! Discarding uncommitted changes — the only operation that destroys work,
//! so it is built to be recoverable:
//!
//! - **Tracked files**: before anything changes, their content is saved as
//!   blobs (exact bytes, mode and symlinks, for an exact undo) and, when
//!   there is a HEAD, as a stash entry "GitCRAFT: descarte de …" that stays
//!   listed in `git stash list` even after GitCRAFT is closed.
//! - **Untracked files**: moved to the Trash, never deleted.
//! - **Undo** puts everything back byte for byte, but only if the files were
//!   not touched since the discard (it never overwrites newer work).
//!
//! Only unstaged changes are discarded; the index is never modified.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ops::status::working_status;
use crate::ops::worktree::{WorktreeArea, worktree_file_diff};
use crate::parse::diff::{LineKind, fingerprint};
use crate::parse::diff_tree::parse_numstat;
use crate::patch::{Direction, HunkSelection, build_patch};
use crate::{CancellationToken, Git, GitError, Result};

/// Moves files to the system Trash, returning where each one ended up.
pub trait Trash: Send + Sync {
    fn trash(&self, path: &Path) -> io::Result<PathBuf>;
}

/// The macOS Trash (Finder's "Put Back" works too).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemTrash;

#[cfg(target_os = "macos")]
impl Trash for SystemTrash {
    fn trash(&self, path: &Path) -> io::Result<PathBuf> {
        use objc2_foundation::{NSFileManager, NSString, NSURL};
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let mut moved_to = None;
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut moved_to))
            .map_err(|e| io::Error::other(e.localizedDescription().to_string()))?;
        moved_to
            .and_then(|url| url.path())
            .map(|p| PathBuf::from(p.to_string()))
            .ok_or_else(|| io::Error::other("the Trash did not report where the file went"))
    }
}

#[cfg(not(target_os = "macos"))]
impl Trash for SystemTrash {
    fn trash(&self, _path: &Path) -> io::Result<PathBuf> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "moving files to the Trash is only implemented on macOS",
        ))
    }
}

/// What to discard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum DiscardTarget {
    /// Whole files: unstaged changes of tracked files, or untracked files
    /// and directories (paths ending in `/`).
    Files { paths: Vec<String> },
    /// Selected hunks/lines of one tracked file's unstaged diff.
    Selection {
        path: String,
        fingerprint: String,
        selection: Vec<HunkSelection>,
    },
}

/// What a discard loses, file by file (shown before confirming).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DiscardedFile {
    pub path: String,
    /// Moved to the Trash instead of reverted.
    pub untracked: bool,
    /// Tracked: added lines that disappear and removed lines that come back
    /// (`None` for binary files).
    pub added: Option<u32>,
    pub removed: Option<u32>,
    /// Untracked file: its number of lines (`None` if binary).
    pub lines: Option<u32>,
    /// Untracked directory: how many files it contains.
    pub file_count: Option<u32>,
}

/// A file's state before the discard and a fingerprint of it right after.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Content {
    Missing,
    File { blob: String, mode: u32 },
    Link(PathBuf),
}

#[derive(Debug, Clone)]
enum Saved {
    Tracked {
        path: String,
        before: Content,
        after: Option<String>,
    },
    Untracked {
        path: String,
        trashed_to: PathBuf,
    },
}

/// Everything needed to undo one discard.
#[derive(Debug, Clone)]
pub struct DiscardRecord {
    /// The stash entry with the backup, if one could be created.
    pub stash: Option<String>,
    saved: Vec<Saved>,
}

fn literal(cmd: crate::GitCommand) -> crate::GitCommand {
    cmd.env("GIT_LITERAL_PATHSPECS", "1")
}

fn check_paths(paths: &[String]) -> Result<()> {
    if paths.is_empty() || paths.iter().any(|p| p.is_empty() || p.contains('\0')) {
        return Err(GitError::Parse("invalid paths".into()));
    }
    Ok(())
}

/// Splits paths into tracked-with-unstaged-changes and untracked, refusing
/// paths with nothing to discard (or in conflict).
async fn classify(
    git: &Git,
    repo: &Path,
    paths: &[String],
    cancel: &CancellationToken,
) -> Result<(Vec<String>, Vec<String>)> {
    let status = working_status(git, repo, cancel).await?;
    let (mut tracked, mut untracked) = (Vec::new(), Vec::new());
    for path in paths {
        if status.unstaged.iter().any(|e| &e.path == path) {
            tracked.push(path.clone());
        } else if status.untracked.iter().any(|e| &e.path == path) {
            untracked.push(path.clone());
        } else {
            return Err(GitError::Parse(format!("nothing to discard in {path}")));
        }
    }
    Ok((tracked, untracked))
}

/// Number of lines of a text file; `None` for binary content.
fn count_lines(bytes: &[u8]) -> Option<u32> {
    if bytes.contains(&0) {
        return None;
    }
    let newlines = bytes.iter().filter(|&&b| b == b'\n').count();
    let partial = usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
    u32::try_from(newlines + partial).ok()
}

fn count_files(dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => count_files(&e.path()),
            _ => 1,
        })
        .sum()
}

/// What discarding `target` would lose.
pub async fn preview_discard(
    git: &Git,
    repo: &Path,
    target: &DiscardTarget,
    cancel: &CancellationToken,
) -> Result<Vec<DiscardedFile>> {
    match target {
        DiscardTarget::Files { paths } => {
            check_paths(paths)?;
            let (tracked, untracked) = classify(git, repo, paths, cancel).await?;
            let mut files = Vec::new();
            if !tracked.is_empty() {
                let out = literal(git.read(repo))
                    .args(["diff", "--numstat", "-z", "--no-ext-diff", "--"])
                    .args(&tracked)
                    .output(cancel)
                    .await?;
                let stats = parse_numstat(&out.stdout)?;
                for path in &tracked {
                    let stat = stats.iter().find(|s| &s.path == path);
                    files.push(DiscardedFile {
                        path: path.clone(),
                        untracked: false,
                        added: stat.and_then(|s| s.additions),
                        removed: stat.and_then(|s| s.deletions),
                        lines: None,
                        file_count: None,
                    });
                }
            }
            for path in &untracked {
                let abs = repo.join(path.trim_end_matches('/'));
                let (lines, file_count) = if abs.is_dir() {
                    (None, Some(count_files(&abs)))
                } else {
                    (
                        std::fs::read(&abs).ok().as_deref().and_then(count_lines),
                        None,
                    )
                };
                files.push(DiscardedFile {
                    path: path.clone(),
                    untracked: true,
                    added: None,
                    removed: None,
                    lines,
                    file_count,
                });
            }
            Ok(files)
        }
        DiscardTarget::Selection {
            path, selection, ..
        } => {
            let diff =
                worktree_file_diff(git, repo, path, None, WorktreeArea::Unstaged, true, cancel)
                    .await?;
            let (mut added, mut removed) = (0u32, 0u32);
            for chosen in selection {
                let Some(hunk) = diff.hunks.get(chosen.hunk as usize) else {
                    continue;
                };
                for (i, line) in hunk.lines.iter().enumerate() {
                    let picked = chosen
                        .lines
                        .as_ref()
                        .is_none_or(|l| l.contains(&u32::try_from(i).unwrap_or(u32::MAX)));
                    match (line.kind, picked) {
                        (LineKind::Added, true) => added += 1,
                        (LineKind::Removed, true) => removed += 1,
                        _ => {}
                    }
                }
            }
            Ok(vec![DiscardedFile {
                path: path.clone(),
                untracked: false,
                added: Some(added),
                removed: Some(removed),
                lines: None,
                file_count: None,
            }])
        }
    }
}

/// Saves the exact content of a working tree file into the object database.
async fn capture(
    git: &Git,
    repo: &Path,
    path: &str,
    cancel: &CancellationToken,
) -> Result<Content> {
    let abs = repo.join(path);
    let meta = match std::fs::symlink_metadata(&abs) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Content::Missing),
        Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink() {
        return Ok(Content::Link(std::fs::read_link(&abs)?));
    }
    let bytes = std::fs::read(&abs)?;
    // --no-filters: the bytes as they are on disk, without clean filters
    // or line-ending conversion, so the undo is exact.
    let out = git
        .write(repo)
        .args(["hash-object", "-w", "--no-filters", "--stdin"])
        .stdin(bytes)
        .output(cancel)
        .await?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    };
    #[cfg(not(unix))]
    let mode = 0o644;
    Ok(Content::File {
        blob: out.stdout_lossy().trim().to_owned(),
        mode,
    })
}

/// A fingerprint of a working tree file as it is now (`None` if missing).
fn snapshot(repo: &Path, path: &str) -> Option<String> {
    let abs = repo.join(path);
    let meta = std::fs::symlink_metadata(&abs).ok()?;
    if meta.file_type().is_symlink() {
        return std::fs::read_link(&abs)
            .ok()
            .map(|t| format!("link:{}", t.display()));
    }
    std::fs::read(&abs).ok().map(|bytes| fingerprint(&bytes))
}

/// Creates a stash entry with the current changes, without touching the
/// working tree or the index. `None` when git cannot (no commit yet).
async fn stash_backup(
    git: &Git,
    repo: &Path,
    label: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let created = git
        .write(repo)
        .args(["stash", "create", label])
        .output(cancel)
        .await;
    let oid = match created {
        Ok(out) => out.stdout_lossy().trim().to_owned(),
        // Before the first commit there is nothing to stash against; the
        // blobs saved by `capture` still allow the undo.
        Err(GitError::Failed { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    if oid.is_empty() {
        return Ok(None);
    }
    git.write(repo)
        .args(["stash", "store", "-m", label, &oid])
        .output(cancel)
        .await?;
    Ok(Some(oid))
}

fn label(paths: &[&str]) -> String {
    const SHOWN: usize = 3;
    let mut text = paths
        .iter()
        .take(SHOWN)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if paths.len() > SHOWN {
        text.push_str(&format!(" y {} más", paths.len() - SHOWN));
    }
    format!("GitCRAFT: descarte de {text}")
}

/// Discards `target`. Returns what was discarded and how to undo it.
pub async fn discard(
    git: &Git,
    repo: &Path,
    target: &DiscardTarget,
    trash: &dyn Trash,
    cancel: &CancellationToken,
) -> Result<(Vec<DiscardedFile>, DiscardRecord)> {
    let files = preview_discard(git, repo, target, cancel).await?;
    let all: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

    let (tracked, untracked, patch) = match target {
        DiscardTarget::Files { paths } => {
            let (tracked, untracked) = classify(git, repo, paths, cancel).await?;
            (tracked, untracked, None)
        }
        DiscardTarget::Selection {
            path,
            fingerprint,
            selection,
        } => {
            let status = working_status(git, repo, cancel).await?;
            if !status.unstaged.iter().any(|e| &e.path == path) {
                return Err(GitError::Parse(format!("nothing to discard in {path}")));
            }
            let diff =
                worktree_file_diff(git, repo, path, None, WorktreeArea::Unstaged, true, cancel)
                    .await?;
            if &diff.fingerprint != fingerprint {
                return Err(GitError::StaleDiff);
            }
            let patch = build_patch(&diff, selection, Direction::Reverse)
                .ok_or_else(|| GitError::Parse("the selection contains no change".into()))?;
            (vec![path.clone()], Vec::new(), Some(patch))
        }
    };

    // 1. Back up everything that is about to change.
    let mut saved = Vec::new();
    let stash = if tracked.is_empty() {
        None
    } else {
        stash_backup(git, repo, &label(&all), cancel).await?
    };
    for path in &tracked {
        saved.push(Saved::Tracked {
            path: path.clone(),
            before: capture(git, repo, path, cancel).await?,
            after: None,
        });
    }

    // 2. Discard.
    if let Some(patch) = patch {
        git.write(repo)
            .args(["apply", "-R", "--whitespace=nowarn", "-"])
            .stdin(patch)
            .output(cancel)
            .await?;
    } else if !tracked.is_empty() {
        literal(git.write(repo))
            .args(["restore", "--worktree", "--"])
            .args(&tracked)
            .output(cancel)
            .await?;
    }
    for item in &mut saved {
        if let Saved::Tracked { path, after, .. } = item {
            *after = snapshot(repo, path);
        }
    }
    for path in &untracked {
        let trashed_to = trash.trash(&repo.join(path.trim_end_matches('/')))?;
        saved.push(Saved::Untracked {
            path: path.clone(),
            trashed_to,
        });
    }
    Ok((files, DiscardRecord { stash, saved }))
}

/// Puts back what `record` discarded, byte for byte. Refuses (changing
/// nothing) if any of those files changed since the discard.
pub async fn undo_discard(
    git: &Git,
    repo: &Path,
    record: &DiscardRecord,
    cancel: &CancellationToken,
) -> Result<()> {
    for item in &record.saved {
        match item {
            Saved::Tracked { path, after, .. } => {
                if &snapshot(repo, path) != after {
                    return Err(GitError::UndoConflict(path.clone()));
                }
            }
            Saved::Untracked { path, trashed_to } => {
                let original = repo.join(path.trim_end_matches('/'));
                if std::fs::symlink_metadata(&original).is_ok() || !trashed_to.exists() {
                    return Err(GitError::UndoConflict(path.clone()));
                }
            }
        }
    }

    // Read every blob first, so a failure cannot leave a half-done undo.
    let mut blobs = BTreeMap::new();
    for item in &record.saved {
        if let Saved::Tracked {
            before: Content::File { blob, .. },
            ..
        } = item
        {
            let out = git
                .read(repo)
                .args(["cat-file", "blob", blob])
                .output(cancel)
                .await?;
            blobs.insert(blob.clone(), out.stdout);
        }
    }

    for item in &record.saved {
        match item {
            Saved::Tracked { path, before, .. } => {
                let abs = repo.join(path);
                match before {
                    Content::Missing => {
                        if std::fs::symlink_metadata(&abs).is_ok() {
                            std::fs::remove_file(&abs)?;
                        }
                    }
                    Content::File { blob, mode } => {
                        if let Some(parent) = abs.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        if std::fs::symlink_metadata(&abs).is_ok_and(|m| m.file_type().is_symlink())
                        {
                            std::fs::remove_file(&abs)?;
                        }
                        std::fs::write(&abs, &blobs[blob])?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            std::fs::set_permissions(&abs, std::fs::Permissions::from_mode(*mode))?;
                        }
                    }
                    Content::Link(target) => {
                        if std::fs::symlink_metadata(&abs).is_ok() {
                            std::fs::remove_file(&abs)?;
                        }
                        #[cfg(unix)]
                        std::os::unix::fs::symlink(target, &abs)?;
                    }
                }
            }
            Saved::Untracked { path, trashed_to } => {
                let original = repo.join(path.trim_end_matches('/'));
                if let Some(parent) = original.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::rename(trashed_to, &original)?;
            }
        }
    }

    // The backup in the stash is redundant now.
    if let Some(stash) = &record.stash {
        let list = git
            .read(repo)
            .args(["stash", "list", "--format=%H"])
            .output(cancel)
            .await?;
        if let Some(index) = list.stdout_lossy().lines().position(|l| l.trim() == stash) {
            git.write(repo)
                .args(["stash", "drop", "--quiet", &format!("stash@{{{index}}}")])
                .output(cancel)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_lines_and_detects_binaries() {
        assert_eq!(count_lines(b""), Some(0));
        assert_eq!(count_lines(b"a\nb\n"), Some(2));
        assert_eq!(count_lines(b"a\nb"), Some(2));
        assert_eq!(count_lines(b"\x00\x01"), None);
    }

    #[test]
    fn labels_stash_entries() {
        assert_eq!(label(&["a.txt"]), "GitCRAFT: descarte de a.txt");
        assert_eq!(
            label(&["a", "b", "c", "d", "e"]),
            "GitCRAFT: descarte de a, b, c y 2 más"
        );
    }
}
