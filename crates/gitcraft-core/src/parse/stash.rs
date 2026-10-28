//! `git stash list` records.

use serde::Serialize;

use crate::{GitError, Result};

/// Fields separated by U+001F, records by NUL (`-z`): selector, commit,
/// reflog message, commit subject, commit time and parents.
pub const STASH_FORMAT: &str = "--format=%gd%x1f%H%x1f%gs%x1f%s%x1f%ct%x1f%P";

/// Prefix of the backups made before discarding changes.
pub const DISCARD_PREFIX: &str = "GitCRAFT: descarte de ";
/// Prefix of the stashes made by "stash and switch".
pub const SWITCH_PREFIX: &str = "GitCRAFT: antes de cambiar a ";

/// Who made a stash entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum StashOrigin {
    /// The user (in GitCRAFT or elsewhere).
    User,
    /// GitCRAFT's backup before discarding changes.
    Discard,
    /// GitCRAFT, saving changes before switching branches.
    BeforeSwitch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StashEntry {
    /// Position in the list (`stash@{index}`); changes as entries come and
    /// go, so operations identify entries by `oid`.
    pub index: u32,
    pub oid: String,
    /// The message, without git's "On <branch>: " prefix.
    pub message: String,
    /// Branch it was made on (`None` when HEAD was detached or unknown).
    pub branch: Option<String>,
    /// Unix seconds.
    #[cfg_attr(feature = "specta", specta(type = i32))]
    pub time: i64,
    /// Also holds untracked files (`--include-untracked`).
    pub untracked: bool,
    pub origin: StashOrigin,
}

/// Splits "On main: msg" / "WIP on main: abc1234 subject" into branch and
/// message. Branch names cannot contain ':'.
fn split_subject(subject: &str) -> Option<(Option<&str>, &str)> {
    let rest = subject
        .strip_prefix("On ")
        .or_else(|| subject.strip_prefix("WIP on "))?;
    let (branch, message) = rest.split_once(": ")?;
    let branch = (branch != "(no branch)").then_some(branch);
    Some((branch, message))
}

/// Parses `git stash list -z` output in [`STASH_FORMAT`].
pub fn parse_stash_list(stdout: &str) -> Result<Vec<StashEntry>> {
    stdout
        .split('\0')
        .filter(|r| !r.is_empty())
        .map(|record| {
            let fields: Vec<&str> = record.split('\u{1f}').collect();
            let [selector, oid, reflog, subject, time, parents] = fields[..] else {
                return Err(GitError::Parse(format!("stash record {record:?}")));
            };
            let index = selector
                .strip_prefix("stash@{")
                .and_then(|s| s.strip_suffix('}'))
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| GitError::Parse(format!("stash selector {selector:?}")))?;
            // `stash push -m` records "On <branch>: <msg>"; `stash store -m`
            // (used for discard backups) records the bare message, and the
            // commit subject then still names the branch.
            let (branch, message) = match split_subject(reflog) {
                Some((branch, message)) => (branch, message),
                None => (split_subject(subject).and_then(|(b, _)| b), reflog),
            };
            let origin = if message.starts_with(DISCARD_PREFIX) {
                StashOrigin::Discard
            } else if message.starts_with(SWITCH_PREFIX) {
                StashOrigin::BeforeSwitch
            } else {
                StashOrigin::User
            };
            Ok(StashEntry {
                index,
                oid: oid.to_owned(),
                message: message.to_owned(),
                branch: branch.map(str::to_owned),
                time: time.trim().parse().unwrap_or(0),
                untracked: parents.split_whitespace().count() == 3,
                origin,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fields: [&str; 6]) -> String {
        fields.join("\u{1f}")
    }

    #[test]
    fn parses_messages_branches_and_origins() {
        let out = [
            record([
                "stash@{0}",
                "a1",
                "On main: GitCRAFT: antes de cambiar a feature",
                "On main: GitCRAFT: antes de cambiar a feature",
                "100",
                "p1 p2 p3",
            ]),
            record([
                "stash@{1}",
                "a2",
                "GitCRAFT: descarte de c.txt",
                "On feature/x: GitCRAFT: descarte de c.txt",
                "90",
                "p1 p2",
            ]),
            record([
                "stash@{2}",
                "a3",
                "WIP on (no branch): 4b4c6f0 base",
                "WIP on (no branch): 4b4c6f0 base",
                "80",
                "p1 p2",
            ]),
            record([
                "stash@{10}",
                "a4",
                "On main: con: dos puntos",
                "On main: con: dos puntos",
                "70",
                "p1 p2",
            ]),
        ]
        .join("\0")
            + "\0";
        let list = parse_stash_list(&out).expect("parse");
        assert_eq!(list.len(), 4);

        assert_eq!(list[0].origin, StashOrigin::BeforeSwitch);
        assert_eq!(list[0].message, "GitCRAFT: antes de cambiar a feature");
        assert_eq!(list[0].branch.as_deref(), Some("main"));
        assert!(list[0].untracked);

        assert_eq!(list[1].origin, StashOrigin::Discard);
        assert_eq!(list[1].message, "GitCRAFT: descarte de c.txt");
        assert_eq!(list[1].branch.as_deref(), Some("feature/x"));
        assert!(!list[1].untracked);

        assert_eq!(list[2].origin, StashOrigin::User);
        assert_eq!(list[2].branch, None);
        assert_eq!(list[2].message, "4b4c6f0 base");

        assert_eq!(list[3].index, 10);
        assert_eq!(list[3].message, "con: dos puntos");
        assert_eq!(list[3].time, 70);
    }

    #[test]
    fn empty_and_malformed() {
        assert!(parse_stash_list("").expect("empty").is_empty());
        assert!(parse_stash_list("stash@{0}\u{1f}x").is_err());
    }
}
