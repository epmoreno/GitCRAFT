//! Tags (task 10.5): creating them (lightweight or annotated), pushing them
//! to a remote, and deleting them, locally or on a remote. Deleting on a
//! remote is never a side effect of deleting locally: it is its own action.

use std::path::Path;

use serde::Serialize;

use crate::ops::remote::{known_remote, parse_push_rejection};
use crate::{CancellationToken, Git, GitError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TagNameCheck {
    Valid,
    /// Not a name git accepts for a tag.
    Invalid,
    /// A tag with that name already exists (it is never moved).
    Exists,
}

/// A tag as the user sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TagInfo {
    pub name: String,
    /// What `refs/tags/<name>` points to (the tag object if annotated).
    pub oid: String,
    /// The commit it marks.
    pub commit: String,
    pub annotated: bool,
}

/// The same tag on a remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RemoteTag {
    pub remote: String,
    /// What the remote's `refs/tags/<name>` points to.
    pub oid: String,
    /// It points to the same object as the local tag.
    pub same: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TagPushOutcome {
    Done,
    /// Refused by the remote (a different tag with that name exists there,
    /// it changed since it was shown, a hook…); nothing changed. `detail`
    /// is git's summary.
    Rejected {
        detail: String,
    },
}

/// Whether git takes `name` as a tag name: a valid ref name that does not
/// start with "-" (`git tag` refuses those although `check-ref-format`
/// accepts them).
async fn valid_name(
    git: &Git,
    repo: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<bool> {
    if name.is_empty() || name.starts_with('-') || name.contains('\0') {
        return Ok(false);
    }
    let out = git
        .read(repo)
        .args(["check-ref-format"])
        .arg(format!("refs/tags/{name}"))
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    Ok(out.exit_code == Some(0))
}

/// The local tag `name`, if there is one.
pub async fn tag_info(
    git: &Git,
    repo: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<Option<TagInfo>> {
    if !valid_name(git, repo, name, cancel).await? {
        return Ok(None);
    }
    let out = git
        .read(repo)
        .args([
            "for-each-ref",
            "--format=%(objectname)%00%(objecttype)%00%(*objectname)",
        ])
        .arg(format!("refs/tags/{name}"))
        .output(cancel)
        .await?;
    let text = out.stdout_lossy();
    let line = text.lines().next().unwrap_or_default();
    let mut parts = line.split('\0');
    let (Some(oid), Some(kind), Some(peeled)) = (parts.next(), parts.next(), parts.next()) else {
        return Ok(None);
    };
    if oid.is_empty() {
        return Ok(None);
    }
    let annotated = kind == "tag";
    Ok(Some(TagInfo {
        name: name.to_owned(),
        oid: oid.to_owned(),
        commit: if annotated { peeled } else { oid }.to_owned(),
        annotated,
    }))
}

pub async fn check_tag_name(
    git: &Git,
    repo: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<TagNameCheck> {
    if !valid_name(git, repo, name, cancel).await? {
        return Ok(TagNameCheck::Invalid);
    }
    Ok(if tag_info(git, repo, name, cancel).await?.is_some() {
        TagNameCheck::Exists
    } else {
        TagNameCheck::Valid
    })
}

/// Creates tag `name` on commit `oid`: annotated with `message` (author and
/// date from the user's git config, like `git tag -a`), or lightweight.
/// Never moves an existing tag.
pub async fn create_tag(
    git: &Git,
    repo: &Path,
    oid: &str,
    name: &str,
    message: Option<&str>,
    cancel: &CancellationToken,
) -> Result<TagInfo> {
    let oid = crate::ops::commit::checked_oid(oid)?;
    match check_tag_name(git, repo, name, cancel).await? {
        TagNameCheck::Invalid => return Err(GitError::InvalidTagName(name.to_owned())),
        TagNameCheck::Exists => return Err(GitError::TagExists(name.to_owned())),
        TagNameCheck::Valid => {}
    }
    let mut cmd = git.write(repo).arg("tag");
    if let Some(message) = message {
        if message.trim().is_empty() {
            return Err(GitError::Failed {
                args: vec!["tag".into()],
                exit_code: None,
                stderr: "empty tag message".into(),
                stdout_tail: String::new(),
            });
        }
        cmd = cmd
            .args(["-a", "-F", "-"])
            // -F never opens an editor; this makes sure no config can.
            .env("GIT_EDITOR", "true")
            .stdin(message.as_bytes().to_vec());
    }
    cmd.arg(name).arg(oid).output(cancel).await?;
    tag_info(git, repo, name, cancel)
        .await?
        .ok_or_else(|| GitError::Parse(format!("tag {name} was not created")))
}

/// Deletes the local tag `name`, only if it still points to `expected`
/// (what the user saw). The remotes are not touched.
pub async fn delete_tag(
    git: &Git,
    repo: &Path,
    name: &str,
    expected: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let expected = crate::ops::commit::checked_oid(expected)?;
    match tag_info(git, repo, name, cancel).await? {
        Some(info) if info.oid == expected => {}
        _ => return Err(GitError::StaleDiff),
    }
    git.write(repo)
        .args(["update-ref", "-d"])
        .arg(format!("refs/tags/{name}"))
        .arg(expected)
        .output(cancel)
        .await?;
    Ok(())
}

/// The remotes that have tag `name` (asked over the network, now).
pub async fn remote_tags(
    git: &Git,
    repo: &Path,
    name: &str,
    cancel: &CancellationToken,
) -> Result<Vec<RemoteTag>> {
    if !valid_name(git, repo, name, cancel).await? {
        return Err(GitError::InvalidTagName(name.to_owned()));
    }
    let local = tag_info(git, repo, name, cancel).await?;
    let mut found = Vec::new();
    for remote in crate::ops::remote::remotes(git, repo, cancel).await? {
        let out = git
            .read(repo)
            .args(["ls-remote", "--tags", "--refs", &remote])
            .arg(format!("refs/tags/{name}"))
            .output(cancel)
            .await?;
        if let Some(oid) = out
            .stdout_lossy()
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .find(|(_, r)| *r == format!("refs/tags/{name}"))
            .map(|(oid, _)| oid.to_owned())
        {
            found.push(RemoteTag {
                same: local.as_ref().is_some_and(|l| l.oid == oid),
                remote,
                oid,
            });
        }
    }
    Ok(found)
}

async fn push_ref(
    git: &Git,
    repo: &Path,
    remote: &str,
    lease: Option<String>,
    refspec: String,
    cancel: &CancellationToken,
) -> Result<TagPushOutcome> {
    known_remote(git, repo, remote, cancel).await?;
    let mut cmd = git.write(repo).args(["push", "--porcelain"]);
    if let Some(lease) = lease {
        cmd = cmd.arg(lease);
    }
    let out = cmd
        .arg(remote)
        .arg(&refspec)
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let stdout = out.stdout_lossy();
    if let Some((_, detail)) = parse_push_rejection(&stdout) {
        return Ok(TagPushOutcome::Rejected { detail });
    }
    if out.exit_code != Some(0) {
        return Err(GitError::Failed {
            args: vec!["push".into(), remote.to_owned(), refspec],
            exit_code: out.exit_code,
            stdout_tail: stdout,
            stderr: out.stderr,
        });
    }
    Ok(TagPushOutcome::Done)
}

/// Pushes the local tag `name` to `remote`. Never forces: a different tag
/// with that name there is reported, not replaced.
pub async fn push_tag(
    git: &Git,
    repo: &Path,
    remote: &str,
    name: &str,
    cancel: &CancellationToken,
) -> Result<TagPushOutcome> {
    if tag_info(git, repo, name, cancel).await?.is_none() {
        return Err(GitError::StaleDiff);
    }
    let full = format!("refs/tags/{name}");
    push_ref(git, repo, remote, None, format!("{full}:{full}"), cancel).await
}

/// Deletes tag `name` on `remote`, only if it still points to `expected`
/// there (`--force-with-lease`): what the user saw when confirming.
pub async fn delete_remote_tag(
    git: &Git,
    repo: &Path,
    remote: &str,
    name: &str,
    expected: &str,
    cancel: &CancellationToken,
) -> Result<TagPushOutcome> {
    if !valid_name(git, repo, name, cancel).await? {
        return Err(GitError::InvalidTagName(name.to_owned()));
    }
    let expected = crate::ops::commit::checked_oid(expected)?;
    let full = format!("refs/tags/{name}");
    push_ref(
        git,
        repo,
        remote,
        Some(format!("--force-with-lease={full}:{expected}")),
        format!(":{full}"),
        cancel,
    )
    .await
}
