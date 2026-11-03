//! Reading and writing git's own configuration (identity, default branch,
//! pull strategy). GitCRAFT keeps no copy: what git has is what is shown.
//!
//! Keys are fixed by GitCRAFT; values come from the user and always go
//! after `--`, so a value such as `-x` is never taken for an option.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{CancellationToken, Git, GitError, Result};

/// Which configuration file a value is read from or written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigTarget {
    /// `~/.gitconfig` (or `$GIT_CONFIG_GLOBAL`): every repository and the
    /// terminal.
    Global,
    /// `.git/config` of this repository.
    Repo(PathBuf),
}

impl ConfigTarget {
    fn flag(&self) -> &'static str {
        match self {
            Self::Global => "--global",
            Self::Repo(_) => "--local",
        }
    }

    fn cwd(&self) -> PathBuf {
        match self {
            // Any folder will do for the global file.
            Self::Global => std::env::temp_dir(),
            Self::Repo(path) => path.clone(),
        }
    }
}

/// One value to write (`Some`) or remove (`None`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ConfigChange {
    pub key: String,
    pub value: Option<String>,
}

/// A value as git resolves it, and where it comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EffectiveValue {
    pub value: String,
    /// `global`, `local`, `system`, `worktree` or `command`.
    pub scope: String,
    /// The file it was read from (`file:<path>`) as git reports it.
    pub origin: String,
}

fn check_value(value: &str) -> Result<()> {
    if value.contains('\0') {
        return Err(GitError::Parse("invalid config value".into()));
    }
    Ok(())
}

/// The value of `key` in `target` only; `None` when it is not set there.
pub async fn get(
    git: &Git,
    target: &ConfigTarget,
    key: &str,
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(target.cwd())
        .args(["config", target.flag(), "-z", "--get", "--", key])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    if out.exit_code != Some(0) {
        return Ok(None);
    }
    let text = out.stdout_lossy();
    Ok(Some(text.strip_suffix('\0').unwrap_or(&text).to_owned()))
}

/// The value git uses for `key` in the repository at `repo`, from any file.
pub async fn effective(
    git: &Git,
    repo: &Path,
    key: &str,
    cancel: &CancellationToken,
) -> Result<Option<EffectiveValue>> {
    let out = git
        .read(repo)
        .args([
            "config",
            "--show-scope",
            "--show-origin",
            "-z",
            "--get",
            "--",
            key,
        ])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    if out.exit_code != Some(0) {
        return Ok(None);
    }
    parse_effective(&out.stdout_lossy()).map(Some)
}

/// `scope NUL origin NUL value NUL`, as `--show-scope --show-origin -z`.
fn parse_effective(text: &str) -> Result<EffectiveValue> {
    let mut parts = text.split('\0');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(scope), Some(origin), Some(value)) => Ok(EffectiveValue {
            value: value.to_owned(),
            scope: scope.to_owned(),
            origin: origin.to_owned(),
        }),
        _ => Err(GitError::Parse(format!(
            "git config --show-origin: {text:?}"
        ))),
    }
}

/// Writes or removes each value in `target`, in order.
pub async fn apply(
    git: &Git,
    target: &ConfigTarget,
    changes: &[ConfigChange],
    cancel: &CancellationToken,
) -> Result<()> {
    for change in changes {
        let cmd = git.write(target.cwd()).args(["config", target.flag()]);
        match &change.value {
            Some(value) => {
                check_value(value)?;
                cmd.args(["--", &change.key, value]).output(cancel).await?;
            }
            // Exit code 5: it was not set, which is what we want.
            None => {
                cmd.args(["--unset-all", "--", &change.key])
                    .accept_exit_code(5)
                    .output(cancel)
                    .await?;
            }
        }
    }
    Ok(())
}

/// What `git pull` does by default, as Settings offers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PullStrategy {
    /// `pull.rebase=false`: a merge commit when both sides have commits.
    Merge,
    /// `pull.rebase=true`: local commits are replayed on top.
    Rebase,
    /// `pull.ff=only`: only when no merge is needed.
    FastForwardOnly,
}

/// The strategy git's values describe; `None` when neither is set (git then
/// merges, and since 2.27 warns about it).
pub fn read_pull_strategy(rebase: Option<&str>, ff: Option<&str>) -> Option<PullStrategy> {
    if ff.is_some_and(|v| v.eq_ignore_ascii_case("only")) {
        return Some(PullStrategy::FastForwardOnly);
    }
    let rebase = rebase?.to_ascii_lowercase();
    Some(
        if matches!(
            rebase.as_str(),
            "true" | "yes" | "on" | "1" | "merges" | "m" | "interactive" | "i"
        ) {
            PullStrategy::Rebase
        } else {
            PullStrategy::Merge
        },
    )
}

/// The values to write for `strategy` (`None` removes both): exactly one
/// of `pull.rebase` / `pull.ff` decides, the other is removed.
pub fn pull_strategy_changes(strategy: Option<PullStrategy>) -> Vec<ConfigChange> {
    let change = |key: &str, value: Option<&str>| ConfigChange {
        key: key.to_owned(),
        value: value.map(str::to_owned),
    };
    match strategy {
        Some(PullStrategy::Merge) => vec![
            change("pull.rebase", Some("false")),
            change("pull.ff", None),
        ],
        Some(PullStrategy::Rebase) => {
            vec![change("pull.rebase", Some("true")), change("pull.ff", None)]
        }
        Some(PullStrategy::FastForwardOnly) => {
            vec![change("pull.ff", Some("only")), change("pull.rebase", None)]
        }
        None => vec![change("pull.rebase", None), change("pull.ff", None)],
    }
}

/// Git's global defaults shown in Settings → General.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GlobalDefaults {
    /// `init.defaultBranch`.
    pub default_branch: Option<String>,
    pub pull_strategy: Option<PullStrategy>,
    /// The raw values, to show what changes.
    pub pull_rebase: Option<String>,
    pub pull_ff: Option<String>,
}

pub async fn global_defaults(git: &Git, cancel: &CancellationToken) -> Result<GlobalDefaults> {
    let global = ConfigTarget::Global;
    let default_branch = get(git, &global, "init.defaultBranch", cancel).await?;
    let pull_rebase = get(git, &global, "pull.rebase", cancel).await?;
    let pull_ff = get(git, &global, "pull.ff", cancel).await?;
    Ok(GlobalDefaults {
        default_branch,
        pull_strategy: read_pull_strategy(pull_rebase.as_deref(), pull_ff.as_deref()),
        pull_rebase,
        pull_ff,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_pull_strategy_like_git() {
        assert_eq!(read_pull_strategy(None, None), None);
        assert_eq!(
            read_pull_strategy(Some("false"), None),
            Some(PullStrategy::Merge)
        );
        assert_eq!(
            read_pull_strategy(Some("merges"), None),
            Some(PullStrategy::Rebase)
        );
        // pull.ff=only wins: a rebase setting does not make it merge.
        assert_eq!(
            read_pull_strategy(Some("true"), Some("only")),
            Some(PullStrategy::FastForwardOnly)
        );
        assert_eq!(
            read_pull_strategy(None, Some("false")),
            None,
            "pull.ff=false alone says nothing about rebasing"
        );
    }

    #[test]
    fn one_key_decides_the_strategy() {
        for strategy in [
            Some(PullStrategy::Merge),
            Some(PullStrategy::Rebase),
            Some(PullStrategy::FastForwardOnly),
            None,
        ] {
            let changes = pull_strategy_changes(strategy);
            let value = |key: &str| {
                changes
                    .iter()
                    .find(|c| c.key == key)
                    .and_then(|c| c.value.as_deref())
            };
            assert_eq!(
                read_pull_strategy(value("pull.rebase"), value("pull.ff")),
                strategy
            );
        }
    }

    #[test]
    fn parses_the_origin_of_a_value() {
        assert_eq!(
            parse_effective("local\0file:.git/config\0Ana Pérez\0").expect("parse"),
            EffectiveValue {
                value: "Ana Pérez".into(),
                scope: "local".into(),
                origin: "file:.git/config".into(),
            }
        );
        assert!(parse_effective("global").is_err());
    }
}
