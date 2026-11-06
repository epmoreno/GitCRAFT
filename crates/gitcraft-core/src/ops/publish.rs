//! Publishing a local repository on GitHub: what would be uploaded (with
//! warnings), then creating the repository with gh and adding `origin`. The
//! push itself is the normal one (`remote::push` with `publish_to`): never
//! forced.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::gh::Gh;
use crate::ops::remote::{current_branch, remotes};
use crate::{CancellationToken, Git, GitError, Result};

/// GitHub warns about files over 50 MB (and refuses them over 100 MB).
pub const LARGE_FILE_BYTES: u64 = 50 * 1000 * 1000;

/// Folders that usually hold build output or downloaded dependencies.
pub const BUILD_DIRS: &[&str] = &["build", "bin", "target", "node_modules", "dist"];

/// At most this many large files and build folders are listed.
const MAX_LISTED: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LargeFile {
    pub path: String,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    pub size: u64,
}

/// What publishing the current branch uploads, and what looks wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PublishPreview {
    pub branch: String,
    pub commits: u32,
    /// Compressed size of everything reachable from HEAD (approximate).
    #[cfg_attr(feature = "specta", specta(type = f64))]
    pub size_bytes: u64,
    /// The last commit has a `.gitignore` at the top.
    pub has_gitignore: bool,
    /// Files over [`LARGE_FILE_BYTES`] anywhere in the history.
    pub large_files: Vec<LargeFile>,
    /// Large files beyond those listed.
    pub more_large_files: u32,
    /// Committed folders named like [`BUILD_DIRS`] (topmost only).
    pub build_dirs: Vec<String>,
    /// Build folders beyond those listed.
    pub more_build_dirs: u32,
}

/// Looks at the current branch's history before publishing it. Fails on a
/// detached HEAD or a repository without commits.
pub async fn preview(
    git: &Git,
    repo: &Path,
    large_file_bytes: u64,
    cancel: &CancellationToken,
) -> Result<PublishPreview> {
    let branch = current_branch(git, repo, cancel).await?;
    let head = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "HEAD^{commit}"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    if head.exit_code != Some(0) {
        return Err(GitError::NoCommits);
    }
    let count = git
        .read(repo)
        .args(["rev-list", "--count", "HEAD"])
        .output(cancel)
        .await?;
    let commits = count
        .stdout_lossy()
        .trim()
        .parse()
        .map_err(|_| GitError::Parse(format!("rev-list --count: {}", count.stdout_lossy())))?;

    let gitignore = git
        .read(repo)
        .args(["ls-tree", "--name-only", "-z", "HEAD", "--", ".gitignore"])
        .output(cancel)
        .await?;
    let dirs = git
        .read(repo)
        .args(["ls-tree", "-r", "-d", "--name-only", "-z", "HEAD"])
        .output(cancel)
        .await?;
    let (size_bytes, large_files, more_large_files) =
        objects(git, repo, large_file_bytes, cancel).await?;
    let (build_dirs, more_build_dirs) = build_dirs(&dirs.stdout_lossy());

    Ok(PublishPreview {
        branch,
        commits,
        size_bytes,
        has_gitignore: !gitignore.stdout.is_empty(),
        large_files,
        more_large_files,
        build_dirs,
        more_build_dirs,
    })
}

/// Keeps the first [`MAX_LISTED`] items; returns how many were left out.
fn cut<T>(items: &mut Vec<T>) -> u32 {
    let more = items.len().saturating_sub(MAX_LISTED);
    items.truncate(MAX_LISTED);
    u32::try_from(more).unwrap_or(u32::MAX)
}

/// The topmost committed folders whose name is a typical build folder, and
/// how many more there are.
fn build_dirs(nul_separated: &str) -> (Vec<String>, u32) {
    let mut found: Vec<String> = Vec::new();
    for dir in nul_separated.split_terminator('\0') {
        let name = dir.rsplit('/').next().unwrap_or(dir);
        let inside_found = found.iter().any(|f| dir.starts_with(&format!("{f}/")));
        if BUILD_DIRS.contains(&name) && !inside_found {
            found.push(dir.to_owned());
        }
    }
    let more = cut(&mut found);
    (found, more)
}

/// Total compressed size of the objects reachable from HEAD, and the blobs
/// larger than `limit` with a path they appear under (the largest first), and
/// how many more there are.
async fn objects(
    git: &Git,
    repo: &Path,
    limit: u64,
    cancel: &CancellationToken,
) -> Result<(u64, Vec<LargeFile>, u32)> {
    let listed = git
        .read(repo)
        .args(["rev-list", "--objects", "HEAD"])
        .output(cancel)
        .await?;
    let listed = listed.stdout_lossy();
    // `<oid>` for commits, `<oid> <path>` for trees and blobs.
    let mut paths: HashMap<&str, &str> = HashMap::new();
    let mut oids = String::new();
    for line in listed.lines() {
        let (oid, path) = line.split_once(' ').unwrap_or((line, ""));
        paths.entry(oid).or_insert(path);
        oids.push_str(oid);
        oids.push('\n');
    }
    let checked = git
        .read(repo)
        .args([
            "cat-file",
            "--batch-check=%(objectname) %(objecttype) %(objectsize) %(objectsize:disk)",
        ])
        .stdin(oids)
        .output(cancel)
        .await?;
    let mut total = 0u64;
    let mut large = Vec::new();
    for line in checked.stdout_lossy().lines() {
        let mut fields = line.split(' ');
        let (Some(oid), Some(kind), Some(size), Some(disk)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let size: u64 = size.parse().unwrap_or(0);
        total += disk.parse::<u64>().unwrap_or(0);
        if kind == "blob" && size > limit {
            large.push(LargeFile {
                path: paths.get(oid).copied().unwrap_or(oid).to_owned(),
                size,
            });
        }
    }
    large.sort_by_key(|f| std::cmp::Reverse(f.size));
    let more = cut(&mut large);
    Ok((total, large, more))
}

/// GitHub repository names: letters, digits, `.`, `-` and `_`, at most 100,
/// not `.` or `..`. GitCRAFT also refuses a leading `-` (it would look like
/// an option on gh's command line).
pub fn valid_repo_name(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && name != "."
        && name != ".."
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// A valid name from a folder name ("Mi Proyecto (2)" → "Mi-Proyecto-2").
pub fn suggest_repo_name(folder: &str) -> String {
    let mut name = String::new();
    for c in folder.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_') {
            name.push(c);
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    let name: String = name
        .trim_matches(|c| c == '-' || c == '.')
        .chars()
        .take(100)
        .collect();
    if valid_repo_name(&name) {
        name
    } else {
        "repository".to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishRequest {
    pub name: String,
    pub description: Option<String>,
    pub private: bool,
}

/// The repository created on GitHub and the `origin` added for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Published {
    /// `owner/name`.
    pub full_name: String,
    /// The repository's web page.
    pub web_url: String,
    /// What `origin` points to.
    pub remote_url: String,
}

/// Creates `request.name` in the signed-in personal account and adds it as
/// `origin`. Checked first, in order, without changing anything: the
/// repository has no remote, a valid session, a valid name, and the name is
/// free in the account (if it exists, nothing at all is done).
pub async fn publish(
    git: &Git,
    gh: &Gh,
    repo: &Path,
    request: &PublishRequest,
    cancel: &CancellationToken,
) -> Result<Published> {
    if !remotes(git, repo, cancel).await?.is_empty() {
        return Err(GitError::HasRemote);
    }
    let gh = gh.clone().in_repo(repo);
    let account = gh
        .accounts(cancel)
        .await?
        .into_iter()
        .find(|a| a.active && a.valid)
        .ok_or(GitError::GhNotLoggedIn)?;
    if !valid_repo_name(&request.name) {
        return Err(GitError::InvalidRepoName(request.name.clone()));
    }
    if gh
        .repo_exists(&account.login, &request.name, cancel)
        .await?
    {
        return Err(GitError::GhRepoExists(format!(
            "{}/{}",
            account.login, request.name
        )));
    }
    let created = gh
        .create_repo(
            &request.name,
            request.description.as_deref(),
            request.private,
            cancel,
        )
        .await?;
    let remote_url = if account.git_protocol == "ssh" {
        created.ssh_url
    } else {
        created.clone_url
    };
    git.write(repo)
        .args(["remote", "add", "--", "origin", &remote_url])
        .output(cancel)
        .await?;
    Ok(Published {
        full_name: created.full_name,
        web_url: created.html_url,
        remote_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_github_repository_names() {
        for ok in ["gitcraft", "Mi.Repo_2", "a-b", "x".repeat(100).as_str()] {
            assert!(valid_repo_name(ok), "{ok}");
        }
        for bad in [
            "",
            ".",
            "..",
            "-x",
            "a b",
            "ñu",
            "a/b",
            "x".repeat(101).as_str(),
        ] {
            assert!(!valid_repo_name(bad), "{bad}");
        }
    }

    #[test]
    fn suggests_a_name_from_the_folder() {
        assert_eq!(suggest_repo_name("GitCRAFT"), "GitCRAFT");
        assert_eq!(suggest_repo_name("Mi Proyecto (2)"), "Mi-Proyecto-2");
        assert_eq!(suggest_repo_name("-raro-"), "raro");
        assert_eq!(suggest_repo_name("ñññ"), "repository");
    }

    #[test]
    fn says_how_many_build_folders_were_left_out() {
        let many: String = (0..25).map(|i| format!("p{i}/dist\0")).collect();
        let (listed, more) = build_dirs(&many);
        assert_eq!((listed.len(), more), (20, 5));
    }

    #[test]
    fn lists_only_the_topmost_build_folders() {
        assert_eq!(
            build_dirs("src\0src/bin\0node_modules\0node_modules/x/dist\0app/target\0targets\0"),
            (
                vec![
                    "src/bin".to_owned(),
                    "node_modules".to_owned(),
                    "app/target".to_owned()
                ],
                0
            )
        );
    }
}
