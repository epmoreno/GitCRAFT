//! Resolving a file in conflict: writing the result, keeping one version,
//! deleting it, marking it resolved, starting it over, and putting a file
//! marked resolved back in conflict.
//!
//! Every function that writes the working tree takes the fingerprint of
//! the file as the user saw it and does nothing if it has changed since.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::ops::conflict::{Stage, Stages, attributes, checked_path, literal, parse_unmerged};
use crate::ops::status::in_progress;
use crate::parse::conflict::{MarkerLine, leftover_markers};
use crate::parse::diff::fingerprint;
use crate::parse::status::InProgress;
use crate::{CancellationToken, Git, GitError, Result};

/// One of the two sides of a conflict, as git's index calls them (stage 2
/// and 3). The UI names them with `ops::in_progress::ConflictSides`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Side {
    Ours,
    Theirs,
}

impl Side {
    fn flag(self) -> &'static str {
        match self {
            Self::Ours => "--ours",
            Self::Theirs => "--theirs",
        }
    }
}

/// Fingerprint of the regular file at `path` (`None`: there is none).
fn current_fingerprint(repo: &Path, path: &str) -> Result<Option<String>> {
    let full = repo.join(path);
    match std::fs::symlink_metadata(&full) {
        Ok(meta) if meta.is_file() => Ok(Some(fingerprint(&std::fs::read(&full)?))),
        _ => Ok(None),
    }
}

/// Refuses with [`GitError::StaleDiff`] if the file is not as the user saw
/// it (`expected: None` means it did not exist).
fn ensure_unchanged(repo: &Path, path: &str, expected: Option<&str>) -> Result<()> {
    if current_fingerprint(repo, path)?.as_deref() != expected {
        return Err(GitError::StaleDiff);
    }
    Ok(())
}

async fn unmerged(
    git: &Git,
    repo: &Path,
    path: &str,
    cancel: &CancellationToken,
) -> Result<Stages> {
    let out = literal(git.read(repo))
        .args(["ls-files", "-u", "-z", "--", path])
        .output(cancel)
        .await?;
    let stages = parse_unmerged(&out.stdout, path)?;
    if stages.base.is_none() && stages.ours.is_none() && stages.theirs.is_none() {
        return Err(GitError::NotConflicted(path.to_owned()));
    }
    Ok(stages)
}

/// Writes `contents` as the file in conflict at `path`, if it is still as
/// the user saw it (`expected` fingerprint). The write is atomic (a
/// temporary file in the same folder, then renamed over) and keeps the
/// file's permissions. The file stays in conflict: see [`mark_resolved`].
/// Returns the new fingerprint.
pub async fn write_resolution(
    git: &Git,
    repo: &Path,
    path: &str,
    contents: &str,
    expected: &str,
    cancel: &CancellationToken,
) -> Result<String> {
    let path = checked_path(path)?;
    unmerged(git, repo, path, cancel).await?;
    let full = repo.join(path);
    let meta = std::fs::symlink_metadata(&full)?;
    if !meta.is_file() {
        return Err(GitError::StaleDiff);
    }
    ensure_unchanged(repo, path, Some(expected))?;
    replace_file(&full, contents.as_bytes(), meta.permissions())?;
    Ok(fingerprint(contents.as_bytes()))
}

fn replace_file(full: &Path, bytes: &[u8], permissions: std::fs::Permissions) -> Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = full.parent().unwrap_or(Path::new("."));
    let name = full
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (tmp, mut file) = loop {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp: PathBuf = dir.join(format!(".{name}.gitcraft-{}-{n}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(file) => break (tmp, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    };
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::set_permissions(&tmp, permissions));
    drop(file);
    let result = written.and_then(|()| std::fs::rename(&tmp, full));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(result?)
}

/// Replaces the file with one side's version (`git checkout --ours` /
/// `--theirs`), for files that cannot be resolved block by block. The file
/// stays in conflict. Returns the new fingerprint.
pub async fn choose_version(
    git: &Git,
    repo: &Path,
    path: &str,
    side: Side,
    expected: Option<&str>,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let path = checked_path(path)?;
    let stages = unmerged(git, repo, path, cancel).await?;
    let exists = match side {
        Side::Ours => stages.ours.is_some(),
        Side::Theirs => stages.theirs.is_some(),
    };
    if !exists {
        return Err(GitError::Parse(format!("{path}: no {side:?} version")));
    }
    ensure_unchanged(repo, path, expected)?;
    literal(git.write(repo))
        .args(["checkout", side.flag(), "--", path])
        .output(cancel)
        .await?;
    current_fingerprint(repo, path)
}

/// Resolves the conflict by deleting the file (`git rm`): for files one
/// side deleted. The other side's version stays in that side's history.
pub async fn resolve_by_deleting(
    git: &Git,
    repo: &Path,
    path: &str,
    expected: Option<&str>,
    cancel: &CancellationToken,
) -> Result<()> {
    let path = checked_path(path)?;
    unmerged(git, repo, path, cancel).await?;
    ensure_unchanged(repo, path, expected)?;
    literal(git.write(repo))
        .args(["rm", "--quiet", "--force", "--ignore-unmatch", "--", path])
        .output(cancel)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum MarkOutcome {
    Resolved,
    /// Nothing was done: the file still has these marker lines.
    MarkersLeft {
        lines: Vec<MarkerLine>,
    },
}

/// Marks the file resolved (`git add`). If it still has conflict markers
/// nothing is done and they are returned, unless `force` (some files
/// contain such lines on purpose).
pub async fn mark_resolved(
    git: &Git,
    repo: &Path,
    path: &str,
    force: bool,
    cancel: &CancellationToken,
) -> Result<MarkOutcome> {
    let path = checked_path(path)?;
    unmerged(git, repo, path, cancel).await?;
    if !force {
        let full = repo.join(path);
        let is_file = std::fs::symlink_metadata(&full).is_ok_and(|m| m.is_file());
        if is_file {
            let bytes = std::fs::read(&full)?;
            if !bytes.contains(&0) {
                let (size, _) = attributes(git, repo, path, cancel).await?;
                let lines = leftover_markers(&String::from_utf8_lossy(&bytes), size);
                if !lines.is_empty() {
                    return Ok(MarkOutcome::MarkersLeft { lines });
                }
            }
        }
    }
    literal(git.write(repo))
        .args(["add", "--all", "--", path])
        .output(cancel)
        .await?;
    Ok(MarkOutcome::Resolved)
}

/// Starts the file over: puts back what git left when the operation
/// stopped (`git checkout -m` rewrites the conflict markers; with one side
/// deleted, the remaining version). Whatever was resolved in the file is
/// lost. Returns the new fingerprint.
pub async fn restart_file(
    git: &Git,
    repo: &Path,
    path: &str,
    expected: Option<&str>,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let path = checked_path(path)?;
    let stages = unmerged(git, repo, path, cancel).await?;
    ensure_unchanged(repo, path, expected)?;
    let how = match (&stages.ours, &stages.theirs) {
        (Some(_), Some(_)) => "-m",
        (Some(_), None) => "--ours",
        (None, Some(_)) => "--theirs",
        (None, None) => return current_fingerprint(repo, path),
    };
    literal(git.write(repo))
        .args(["checkout", how, "--", path])
        .output(cancel)
        .await?;
    current_fingerprint(repo, path)
}

/// A file marked resolved during the current operation that can be put
/// back in conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ResolvedFile {
    pub path: String,
    /// The file was edited after being marked resolved: putting it back in
    /// conflict forgets the version that was marked (the file itself stays
    /// as it is).
    pub changed_since: bool,
}

/// `<mode> <oid> <stage>\t<path>` records (`ls-files -u` and
/// `--resolve-undo`), grouped by path.
fn stage_map(stdout: &[u8]) -> Result<BTreeMap<String, Stages>> {
    let mut paths = BTreeMap::new();
    for record in stdout.split(|&b| b == 0).filter(|r| !r.is_empty()) {
        let text = String::from_utf8_lossy(record);
        let path = text
            .split_once('\t')
            .map(|(_, p)| p.to_owned())
            .ok_or_else(|| GitError::Parse(format!("stage record: {text:?}")))?;
        if paths.contains_key(&path) {
            continue;
        }
        let stages = parse_unmerged(stdout, &path)?;
        paths.insert(path, stages);
    }
    Ok(paths)
}

async fn blob_at(
    git: &Git,
    repo: &Path,
    rev: &str,
    path: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "--end-of-options"])
        .arg(format!("{rev}:{path}"))
        .accept_exit_code(1)
        .accept_exit_code(128)
        .output(cancel)
        .await?;
    let oid = out.stdout_lossy().trim().to_owned();
    Ok((out.exit_code == Some(0) && !oid.is_empty()).then_some(oid))
}

/// The commit whose changes stage 3 holds in the current operation.
fn their_rev(operation: InProgress) -> &'static str {
    match operation {
        InProgress::Merge => "MERGE_HEAD",
        InProgress::Rebase => "REBASE_HEAD",
        InProgress::CherryPick => "CHERRY_PICK_HEAD",
        // A revert applies the commit's changes backwards: "theirs" is its
        // parent.
        InProgress::Revert => "REVERT_HEAD^",
    }
}

/// Whether the saved versions of `path` are those of the current operation:
/// git keeps them after the operation ends and across later ones, and
/// restoring old ones would invent a conflict.
async fn belongs_to_operation(
    git: &Git,
    repo: &Path,
    operation: InProgress,
    path: &str,
    saved: &Stages,
    cancel: &CancellationToken,
) -> Result<bool> {
    let oid = |s: &Option<Stage>| s.as_ref().map(|s| s.oid.clone());
    if saved.ours.is_none() && saved.theirs.is_none() {
        return Ok(false);
    }
    let ours = blob_at(git, repo, "HEAD", path, cancel).await?;
    let theirs = blob_at(git, repo, their_rev(operation), path, cancel).await?;
    Ok(oid(&saved.ours) == ours && oid(&saved.theirs) == theirs)
}

/// Files marked resolved during the operation in progress that git can put
/// back in conflict (it keeps their versions as "resolve-undo" data). Empty
/// when nothing is in progress.
pub async fn resolved_files(
    git: &Git,
    repo: &Path,
    cancel: &CancellationToken,
) -> Result<Vec<ResolvedFile>> {
    let Some(operation) = in_progress(git, repo, cancel).await? else {
        return Ok(Vec::new());
    };
    // Older gits have no `--resolve-undo` listing (exit 129): then nothing
    // is offered.
    let saved = git
        .read(repo)
        .args(["ls-files", "-z", "--resolve-undo"])
        .accept_exit_code(129)
        .output(cancel)
        .await?;
    if saved.exit_code != Some(0) {
        return Ok(Vec::new());
    }
    let unmerged = git
        .read(repo)
        .args(["ls-files", "-z", "-u"])
        .output(cancel)
        .await?;
    let still_unmerged = stage_map(&unmerged.stdout)?;
    let mut files = Vec::new();
    for (path, stages) in stage_map(&saved.stdout)? {
        if still_unmerged.contains_key(&path)
            || !belongs_to_operation(git, repo, operation, &path, &stages, cancel).await?
        {
            continue;
        }
        let diff = literal(git.read(repo))
            .args(["diff", "--quiet", "--", &path])
            .accept_exit_code(1)
            .output(cancel)
            .await?;
        files.push(ResolvedFile {
            changed_since: diff.exit_code == Some(1),
            path,
        });
    }
    Ok(files)
}

/// Puts a file marked resolved back in conflict (`git update-index
/// --unresolve`): the index gets back its versions from when the operation
/// stopped; the file in the working tree is not touched. Only for the files
/// [`resolved_files`] lists; anything else fails with
/// [`GitError::CannotUnmark`] and nothing is done.
pub async fn unmark_resolved(
    git: &Git,
    repo: &Path,
    path: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let path = checked_path(path)?;
    let listed = resolved_files(git, repo, cancel).await?;
    if !listed.iter().any(|f| f.path == path) {
        return Err(GitError::CannotUnmark(path.to_owned()));
    }
    let saved = git
        .read(repo)
        .args(["ls-files", "-z", "--resolve-undo"])
        .output(cancel)
        .await?;
    let expected = parse_unmerged(&saved.stdout, path)?;
    git.write(repo)
        .args(["update-index", "--unresolve", "--", path])
        .output(cancel)
        .await?;
    let restored = unmerged(git, repo, path, cancel).await?;
    if restored.base != expected.base
        || restored.ours != expected.ours
        || restored.theirs != expected.theirs
    {
        return Err(GitError::Parse(format!(
            "{path}: the versions put back are not the saved ones"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_records_grouped_by_path() {
        let h = "b".repeat(40);
        let out = format!("100644 {h} 1\ta.txt\0100644 {h} 2\ta.txt\0100644 {h} 3\tz z.txt\0");
        let map = stage_map(out.as_bytes()).expect("parse");
        assert_eq!(map.len(), 2);
        assert!(map["a.txt"].base.is_some() && map["a.txt"].theirs.is_none());
        assert!(map["z z.txt"].theirs.is_some());
    }

    #[test]
    fn replace_keeps_permissions_and_leaves_no_temp() {
        let dir = tempfile_dir();
        let file = dir.join("f.txt");
        std::fs::write(&file, b"old").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        let perms = std::fs::metadata(&file).expect("meta").permissions();
        replace_file(&file, b"new\r\n", perms.clone()).expect("replace");
        assert_eq!(std::fs::read(&file).expect("read"), b"new\r\n");
        assert_eq!(std::fs::metadata(&file).expect("meta").permissions(), perms);
        let names: Vec<_> = std::fs::read_dir(&dir)
            .expect("list")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("f.txt")]);
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    fn tempfile_dir() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gitcraft-resolve-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }
}
