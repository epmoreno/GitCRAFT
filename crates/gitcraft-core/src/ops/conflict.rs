//! Reading one file in conflict: which versions git has of it, whether it
//! can be resolved block by block, and its blocks. Nothing here writes to
//! the repository.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

use crate::parse::conflict::{
    ConflictBlock, DEFAULT_MARKER_SIZE, MalformedMarkers, Segment, parse_conflicts,
};
use crate::parse::diff::fingerprint;
use crate::parse::status::ConflictKind;
use crate::{CancellationToken, Git, GitCommand, GitError, Result};

/// Bigger files (any version) are only offered "keep one version whole":
/// block views of them would be slow and hard to read.
pub const MAX_BLOCK_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// Bytes git looks at to decide whether content is binary.
const BINARY_PROBE: usize = 8000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum EntryKind {
    File,
    Executable,
    Symlink,
    Submodule,
}

/// One version of the file in the index (stage 1, 2 or 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ConflictVersion {
    pub oid: String,
    pub kind: EntryKind,
    /// `None` for a submodule (its commit is not an object of this repo).
    #[cfg_attr(feature = "specta", specta(type = Option<f64>))]
    pub size: Option<u64>,
}

/// Why a file can only be resolved by keeping one version whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum WholeFileReason {
    /// Binary content, or the `merge` attribute says not to merge text.
    Binary,
    /// Not valid UTF-8: editing it as text could change its bytes.
    NotUtf8,
    Symlink,
    Submodule,
    /// Over [`MAX_BLOCK_FILE_BYTES`].
    TooLarge,
    /// One side deleted (or renamed away) the file.
    OneSideMissing,
    /// Both sides deleted it.
    BothDeleted,
    /// There is no regular file at the path in the working tree.
    NotInWorktree,
    /// The markers do not form complete blocks (1-based line).
    MalformedMarkers {
        line: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ConflictContent {
    /// The working tree file, split into common text and conflict blocks.
    /// Blocks whose base could not be known have `base: None`.
    Blocks {
        segments: Vec<Segment>,
        marker_size: u32,
    },
    WholeFile {
        reason: WholeFileReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ConflictFile {
    pub path: String,
    pub kind: ConflictKind,
    /// The common ancestor's version (`None`: both sides added the file).
    pub base: Option<ConflictVersion>,
    pub ours: Option<ConflictVersion>,
    pub theirs: Option<ConflictVersion>,
    /// Identifies the working tree file as read (`None`: no regular file),
    /// so a later write only happens if it has not changed since.
    pub fingerprint: Option<String>,
    pub content: ConflictContent,
}

/// A path from the UI: relative to the repository, inside it, and never
/// read by git as a pathspec.
pub(crate) fn checked_path(path: &str) -> Result<&str> {
    let invalid = || GitError::Parse(format!("invalid path: {path:?}"));
    if path.is_empty() || path.contains('\0') {
        return Err(invalid());
    }
    if !Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(invalid());
    }
    Ok(path)
}

pub(crate) fn literal(cmd: GitCommand) -> GitCommand {
    cmd.env("GIT_LITERAL_PATHSPECS", "1")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stage {
    pub(crate) mode: String,
    pub(crate) oid: String,
}

#[derive(Debug, Default)]
pub(crate) struct Stages {
    pub(crate) base: Option<Stage>,
    pub(crate) ours: Option<Stage>,
    pub(crate) theirs: Option<Stage>,
}

/// `git ls-files -u -z`: `<mode> <oid> <stage>\t<path>` records. Only the
/// records of exactly `path` count (a directory would list its files).
pub(crate) fn parse_unmerged(stdout: &[u8], path: &str) -> Result<Stages> {
    let mut stages = Stages::default();
    for record in stdout.split(|&b| b == 0).filter(|r| !r.is_empty()) {
        let record = String::from_utf8_lossy(record);
        let bad = || GitError::Parse(format!("ls-files -u record: {record:?}"));
        let (info, file) = record.split_once('\t').ok_or_else(bad)?;
        if file != path {
            continue;
        }
        let mut parts = info.split(' ');
        let (Some(mode), Some(oid), Some(stage), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        let entry = Some(Stage {
            mode: mode.to_owned(),
            oid: oid.to_owned(),
        });
        match stage {
            "1" => stages.base = entry,
            "2" => stages.ours = entry,
            "3" => stages.theirs = entry,
            _ => return Err(bad()),
        }
    }
    Ok(stages)
}

fn kind_of(stages: &Stages) -> Option<ConflictKind> {
    Some(
        match (
            stages.base.is_some(),
            stages.ours.is_some(),
            stages.theirs.is_some(),
        ) {
            (true, true, true) => ConflictKind::BothModified,
            (false, true, true) => ConflictKind::BothAdded,
            (true, false, false) => ConflictKind::BothDeleted,
            (false, true, false) => ConflictKind::AddedByUs,
            (false, false, true) => ConflictKind::AddedByThem,
            (true, false, true) => ConflictKind::DeletedByUs,
            (true, true, false) => ConflictKind::DeletedByThem,
            (false, false, false) => return None,
        },
    )
}

fn entry_kind(mode: &str) -> EntryKind {
    match mode {
        "120000" => EntryKind::Symlink,
        "160000" => EntryKind::Submodule,
        "100755" => EntryKind::Executable,
        _ => EntryKind::File,
    }
}

/// `conflict-marker-size` and `merge` attributes of `path`.
pub(crate) async fn attributes(
    git: &Git,
    repo: &Path,
    path: &str,
    cancel: &CancellationToken,
) -> Result<(usize, Option<String>)> {
    let out = git
        .read(repo)
        .args([
            "check-attr",
            "-z",
            "conflict-marker-size",
            "merge",
            "--",
            path,
        ])
        .output(cancel)
        .await?;
    let text = out.stdout_lossy();
    let fields: Vec<&str> = text.split('\0').collect();
    let mut size = DEFAULT_MARKER_SIZE;
    let mut merge = None;
    for record in fields.chunks(3) {
        let [_, attr, value] = record else { continue };
        match *attr {
            "conflict-marker-size" => {
                size = value
                    .parse::<usize>()
                    .ok()
                    .filter(|&n| n > 0)
                    .unwrap_or(DEFAULT_MARKER_SIZE);
            }
            "merge" if *value != "unspecified" => merge = Some((*value).to_owned()),
            _ => {}
        }
    }
    Ok((size, merge))
}

/// Sizes of the given blobs, in order.
async fn blob_sizes(
    git: &Git,
    repo: &Path,
    oids: &[&str],
    cancel: &CancellationToken,
) -> Result<Vec<u64>> {
    if oids.is_empty() {
        return Ok(Vec::new());
    }
    let input: String = oids.iter().map(|oid| format!("{oid}\n")).collect();
    let out = git
        .read(repo)
        .args(["cat-file", "--batch-check=%(objectsize)"])
        .stdin(input)
        .output(cancel)
        .await?;
    let text = out.stdout_lossy();
    let sizes: Vec<u64> = text
        .lines()
        .map(|l| l.trim().parse::<u64>())
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| GitError::Parse(format!("cat-file sizes: {text:?}")))?;
    if sizes.len() != oids.len() {
        return Err(GitError::Parse(format!("cat-file sizes: {text:?}")));
    }
    Ok(sizes)
}

/// A blob as it would be checked out at `path` (line endings and other
/// filters applied), so it compares with the working tree file.
async fn checked_out_blob(
    git: &Git,
    repo: &Path,
    path: &str,
    oid: &str,
    cancel: &CancellationToken,
) -> Result<Vec<u8>> {
    let out = git
        .read(repo)
        .arg("cat-file")
        .arg("--filters")
        .arg(format!("--path={path}"))
        .arg(oid)
        .output(cancel)
        .await?;
    Ok(out.stdout)
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_PROBE)].contains(&0)
}

fn whole(reason: WholeFileReason) -> ConflictContent {
    ConflictContent::WholeFile { reason }
}

/// Reads the file in conflict at `path` (relative to the repository root).
/// Fails with [`GitError::NotConflicted`] if it is not in conflict.
pub async fn conflict_file(
    git: &Git,
    repo: &Path,
    path: &str,
    cancel: &CancellationToken,
) -> Result<ConflictFile> {
    let path = checked_path(path)?;
    let out = literal(git.read(repo))
        .args(["ls-files", "-u", "-z", "--", path])
        .output(cancel)
        .await?;
    let stages = parse_unmerged(&out.stdout, path)?;
    let kind = kind_of(&stages).ok_or_else(|| GitError::NotConflicted(path.to_owned()))?;

    let all = [&stages.base, &stages.ours, &stages.theirs];
    let blobs: Vec<&str> = all
        .iter()
        .filter_map(|s| s.as_ref())
        .filter(|s| entry_kind(&s.mode) != EntryKind::Submodule)
        .map(|s| s.oid.as_str())
        .collect();
    let mut sizes = blob_sizes(git, repo, &blobs, cancel).await?.into_iter();
    let version = |stage: &Option<Stage>, sizes: &mut std::vec::IntoIter<u64>| {
        stage.as_ref().map(|s| {
            let kind = entry_kind(&s.mode);
            ConflictVersion {
                oid: s.oid.clone(),
                kind,
                size: (kind != EntryKind::Submodule)
                    .then(|| sizes.next())
                    .flatten(),
            }
        })
    };
    let base = version(&stages.base, &mut sizes);
    let ours = version(&stages.ours, &mut sizes);
    let theirs = version(&stages.theirs, &mut sizes);

    let on_disk = repo.join(path);
    let meta = std::fs::symlink_metadata(&on_disk).ok();
    let bytes = match &meta {
        Some(m) if m.is_file() && m.len() <= MAX_BLOCK_FILE_BYTES => Some(std::fs::read(&on_disk)?),
        _ => None,
    };
    let file_fingerprint = match (&meta, &bytes) {
        (_, Some(bytes)) => Some(fingerprint(bytes)),
        (Some(m), None) if m.is_file() => Some(fingerprint(&std::fs::read(&on_disk)?)),
        _ => None,
    };

    let mut file = ConflictFile {
        path: path.to_owned(),
        kind,
        base,
        ours,
        theirs,
        fingerprint: file_fingerprint,
        content: whole(WholeFileReason::Binary),
    };
    file.content = classify(git, repo, path, &file, meta.as_ref(), bytes, cancel).await?;
    Ok(file)
}

async fn classify(
    git: &Git,
    repo: &Path,
    path: &str,
    file: &ConflictFile,
    meta: Option<&std::fs::Metadata>,
    bytes: Option<Vec<u8>>,
    cancel: &CancellationToken,
) -> Result<ConflictContent> {
    let versions = [file.base.as_ref(), file.ours.as_ref(), file.theirs.as_ref()];
    let any = |kind: EntryKind| versions.iter().flatten().any(|v| v.kind == kind);
    if any(EntryKind::Submodule) {
        return Ok(whole(WholeFileReason::Submodule));
    }
    if any(EntryKind::Symlink) {
        return Ok(whole(WholeFileReason::Symlink));
    }
    let (Some(ours), Some(theirs)) = (&file.ours, &file.theirs) else {
        return Ok(whole(if file.ours.is_none() && file.theirs.is_none() {
            WholeFileReason::BothDeleted
        } else {
            WholeFileReason::OneSideMissing
        }));
    };
    let (marker_size, merge) = attributes(git, repo, path, cancel).await?;
    if matches!(merge.as_deref(), Some("unset" | "binary")) {
        return Ok(whole(WholeFileReason::Binary));
    }
    if versions
        .iter()
        .flatten()
        .any(|v| v.size.unwrap_or(0) > MAX_BLOCK_FILE_BYTES)
    {
        return Ok(whole(WholeFileReason::TooLarge));
    }
    let Some(meta) = meta.filter(|m| m.is_file()) else {
        return Ok(whole(if meta.is_some_and(|m| m.file_type().is_symlink()) {
            WholeFileReason::Symlink
        } else {
            WholeFileReason::NotInWorktree
        }));
    };
    let Some(bytes) = bytes else {
        debug_assert!(meta.len() > MAX_BLOCK_FILE_BYTES);
        return Ok(whole(WholeFileReason::TooLarge));
    };

    let base_bytes = match &file.base {
        Some(v) => checked_out_blob(git, repo, path, &v.oid, cancel).await?,
        None => Vec::new(),
    };
    let ours_bytes = checked_out_blob(git, repo, path, &ours.oid, cancel).await?;
    let theirs_bytes = checked_out_blob(git, repo, path, &theirs.oid, cancel).await?;
    if [&bytes, &base_bytes, &ours_bytes, &theirs_bytes]
        .iter()
        .any(|b| looks_binary(b))
    {
        return Ok(whole(WholeFileReason::Binary));
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(whole(WholeFileReason::NotUtf8));
    };
    let mut segments = match parse_conflicts(&text, marker_size) {
        Ok(segments) => segments,
        Err(MalformedMarkers { line }) => {
            return Ok(whole(WholeFileReason::MalformedMarkers { line }));
        }
    };

    let needs_base = segments
        .iter()
        .any(|s| matches!(s, Segment::Conflict { block } if block.base.is_none()));
    if needs_base {
        let computed = computed_blocks(
            git,
            repo,
            [&ours_bytes, &base_bytes, &theirs_bytes],
            marker_size,
            cancel,
        )
        .await?;
        attach_bases(&mut segments, &computed);
    }

    Ok(ConflictContent::Blocks {
        segments,
        marker_size: u32::try_from(marker_size).unwrap_or(u32::MAX),
    })
}

/// The blocks `git merge-file --diff3` makes from the three versions (with
/// a base in each), or none if it cannot be done.
async fn computed_blocks(
    git: &Git,
    repo: &Path,
    [ours, base, theirs]: [&[u8]; 3],
    marker_size: usize,
    cancel: &CancellationToken,
) -> Result<Vec<ConflictBlock>> {
    let dir = PrivateTempDir::new()?;
    let names = ["ours", "base", "theirs"];
    for (name, bytes) in names.iter().zip([ours, base, theirs]) {
        std::fs::write(dir.path().join(name), bytes)?;
    }
    let mut cmd = git
        .read(repo)
        .args(["merge-file", "-p", "--diff3"])
        .arg(format!("--marker-size={marker_size}"))
        .args(["-L", "ours", "-L", "base", "-L", "theirs", "--"]);
    for name in names {
        cmd = cmd.arg(dir.path().join(name));
    }
    // The exit code is the number of conflicts (at most 127).
    for code in 1..=127 {
        cmd = cmd.accept_exit_code(code);
    }
    let out = cmd.output(cancel).await?;
    let Ok(text) = String::from_utf8(out.stdout) else {
        return Ok(Vec::new());
    };
    Ok(parse_conflicts(&text, marker_size)
        .map(|segments| {
            segments
                .into_iter()
                .filter_map(|s| match s {
                    Segment::Conflict { block } => Some(block),
                    Segment::Common { .. } => None,
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Gives each block without a base the base of the next computed block
/// with exactly the same two sides, in order. A block with no match (the
/// file was edited, or git merged with other options) keeps `None`.
fn attach_bases(segments: &mut [Segment], computed: &[ConflictBlock]) {
    let mut next = 0;
    for segment in segments.iter_mut() {
        let Segment::Conflict { block } = segment else {
            continue;
        };
        let found = computed[next.min(computed.len())..]
            .iter()
            .position(|c| c.ours == block.ours && c.theirs == block.theirs);
        if let Some(offset) = found {
            let matched = &computed[next + offset];
            if block.base_marker.is_none() {
                block.base.clone_from(&matched.base);
            }
            next += offset + 1;
        }
    }
}

/// A folder only this user can read, removed (with its contents) on drop.
struct PrivateTempDir(PathBuf);

impl PrivateTempDir {
    fn new() -> Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "gitcraft-conflict-{}-{nanos}-{n}",
                std::process::id()
            ));
            #[cfg_attr(not(unix), allow(unused_mut))]
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            match builder.create(&dir) {
                Ok(()) => return Ok(Self(dir)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PrivateTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_must_stay_inside_the_repo() {
        for bad in ["", "../x", "a/../../x", "/etc/passwd", "./a", "a\0b"] {
            assert!(checked_path(bad).is_err(), "{bad:?}");
        }
        for good in ["a.txt", "dir/sub/-x.txt", "con espacios/ñ.txt"] {
            assert_eq!(checked_path(good).ok(), Some(good));
        }
    }

    #[test]
    fn unmerged_records() {
        let h = "a".repeat(40);
        let out = format!(
            "100644 {h} 1\tf.txt\0100755 {h} 2\tf.txt\0100644 {h} 3\tf.txt\0100644 {h} 2\tf.txt.other\0"
        );
        let stages = parse_unmerged(out.as_bytes(), "f.txt").expect("parse");
        assert_eq!(stages.ours.map(|s| s.mode).as_deref(), Some("100755"));
        assert!(stages.base.is_some() && stages.theirs.is_some());
        assert!(parse_unmerged(b"garbage\0", "f.txt").is_err());
    }

    #[test]
    fn bases_attach_in_order_and_only_on_exact_match() {
        let block = |ours: &str, theirs: &str, base: Option<&str>| ConflictBlock {
            ours: ours.into(),
            base: base.map(Into::into),
            theirs: theirs.into(),
            ours_marker: "<<<<<<< a\n".into(),
            base_marker: None,
            separator: "=======\n".into(),
            theirs_marker: ">>>>>>> b\n".into(),
        };
        let mut segments = vec![
            Segment::Conflict {
                block: block("1\n", "2\n", None),
            },
            Segment::Common { text: "x\n".into() },
            Segment::Conflict {
                block: block("edited\n", "4\n", None),
            },
            Segment::Conflict {
                block: block("5\n", "6\n", None),
            },
        ];
        let computed = [
            block("1\n", "2\n", Some("b1\n")),
            block("3\n", "4\n", Some("b2\n")),
            block("5\n", "6\n", Some("b3\n")),
        ];
        attach_bases(&mut segments, &computed);
        let bases: Vec<Option<&str>> = segments
            .iter()
            .filter_map(|s| match s {
                Segment::Conflict { block } => Some(block.base.as_deref()),
                Segment::Common { .. } => None,
            })
            .collect();
        assert_eq!(bases, vec![Some("b1\n"), None, Some("b3\n")]);
    }

    #[test]
    fn temp_dir_is_removed() {
        let dir = PrivateTempDir::new().expect("create");
        let path = dir.path().to_path_buf();
        std::fs::write(path.join("f"), b"x").expect("write");
        drop(dir);
        assert!(!path.exists());
    }
}
