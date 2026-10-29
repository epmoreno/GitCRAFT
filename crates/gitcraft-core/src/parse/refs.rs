//! `git for-each-ref` records and HEAD state.

use serde::Serialize;

use crate::{GitError, Result};

/// Fields separated by U+001F, one ref per line. Ref names cannot contain
/// control characters (see git-check-ref-format), so newlines are safe.
pub const REFS_FORMAT: &str =
    "--format=%(refname)%1f%(objectname)%1f%(*objectname)%1f%(HEAD)%1f%(symref)%1f%(upstream)";

/// The ref namespaces shown in the UI.
pub const REF_PATTERNS: [&str; 3] = ["refs/heads", "refs/remotes", "refs/tags"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GitRef {
    /// Full name, e.g. `refs/remotes/origin/main`.
    pub name: String,
    /// Display name: `main`, `origin/main`, `v1.0`.
    pub short_name: String,
    pub kind: RefKind,
    /// The commit it points to (annotated tags are peeled).
    pub target: String,
    /// For remote branches: the remote's name.
    pub remote: Option<String>,
    /// For local branches: the tracked branch, e.g. `origin/main`.
    pub upstream: Option<String>,
    /// The branch HEAD points to.
    pub is_head: bool,
    /// For local branches with an upstream: commits only on this branch
    /// (to push) and only on the upstream (to pull). `None` when there is
    /// no upstream, it is gone, or it was not counted.
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    /// The upstream is configured but its remote branch no longer exists
    /// (deleted on the remote and pruned).
    pub upstream_gone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Head {
    /// On a branch. `oid` is `None` while the branch has no commits yet.
    Branch {
        name: String,
        oid: Option<String>,
    },
    Detached {
        oid: String,
    },
}

/// Parses `for-each-ref` output. `remotes` (from `git remote`) resolves
/// remote names that contain slashes. Symbolic refs such as `origin/HEAD`
/// are skipped.
pub fn parse_refs(stdout: &str, remotes: &[String]) -> Result<Vec<GitRef>> {
    let mut refs = Vec::new();
    for line in stdout.lines().filter(|l| !l.is_empty()) {
        let fields: Vec<&str> = line.split('\u{1f}').collect();
        let [name, object, peeled, head_marker, symref, upstream] = fields[..] else {
            return Err(GitError::Parse(format!("for-each-ref line: {line:?}")));
        };
        if !symref.is_empty() {
            continue;
        }
        let (kind, short_name) = if let Some(short) = name.strip_prefix("refs/heads/") {
            (RefKind::Local, short)
        } else if let Some(short) = name.strip_prefix("refs/remotes/") {
            (RefKind::Remote, short)
        } else if let Some(short) = name.strip_prefix("refs/tags/") {
            (RefKind::Tag, short)
        } else {
            continue;
        };
        let target = if peeled.is_empty() { object } else { peeled };
        refs.push(GitRef {
            name: name.to_owned(),
            short_name: short_name.to_owned(),
            kind,
            target: target.to_owned(),
            remote: (kind == RefKind::Remote).then(|| remote_of(short_name, remotes)),
            upstream: (!upstream.is_empty()).then(|| shorten(upstream)),
            is_head: head_marker == "*",
            ahead: None,
            behind: None,
            upstream_gone: false,
        });
    }
    Ok(refs)
}

/// The remote a `refs/remotes/<short>` ref belongs to: the longest known
/// remote name that prefixes it, else the first path segment.
fn remote_of(short: &str, remotes: &[String]) -> String {
    remotes
        .iter()
        .filter(|r| {
            short
                .strip_prefix(r.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|r| r.len())
        .cloned()
        .unwrap_or_else(|| short.split('/').next().unwrap_or(short).to_owned())
}

fn shorten(full: &str) -> String {
    full.strip_prefix("refs/remotes/")
        .or_else(|| full.strip_prefix("refs/heads/"))
        .unwrap_or(full)
        .to_owned()
}

/// Builds HEAD from `git symbolic-ref -q HEAD` and `git rev-parse -q --verify HEAD`
/// (each empty when it does not apply).
pub fn parse_head(symbolic_ref: &str, oid: &str) -> Result<Head> {
    let symbolic_ref = symbolic_ref.trim();
    let oid = oid.trim();
    let oid = (!oid.is_empty()).then(|| oid.to_owned());
    match (symbolic_ref.is_empty(), oid) {
        (false, oid) => Ok(Head::Branch {
            name: shorten(symbolic_ref),
            oid,
        }),
        (true, Some(oid)) => Ok(Head::Detached { oid }),
        (true, None) => Err(GitError::Parse(
            "HEAD is neither a branch nor a commit".into(),
        )),
    }
}

/// `git remote` output: one name per line.
pub fn parse_remotes(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "1111111111111111111111111111111111111111";
    const T: &str = "2222222222222222222222222222222222222222";

    fn line(fields: [&str; 6]) -> String {
        fields.join("\u{1f}")
    }

    #[test]
    fn classifies_and_shortens_refs() {
        let out = [
            line([
                "refs/heads/main",
                A,
                "",
                "*",
                "",
                "refs/remotes/origin/main",
            ]),
            line(["refs/heads/feature/x", A, "", " ", "", ""]),
            line(["refs/remotes/origin/main", A, "", " ", "", ""]),
            line([
                "refs/remotes/origin/HEAD",
                A,
                "",
                " ",
                "refs/remotes/origin/main",
                "",
            ]),
            line(["refs/tags/v1.0", T, A, " ", "", ""]),
            line(["refs/tags/light", A, "", " ", "", ""]),
        ]
        .join("\n");
        let refs = parse_refs(&out, &["origin".into()]).expect("parse");

        let names: Vec<_> = refs
            .iter()
            .map(|r| (r.short_name.as_str(), r.kind))
            .collect();
        assert_eq!(
            names,
            [
                ("main", RefKind::Local),
                ("feature/x", RefKind::Local),
                ("origin/main", RefKind::Remote),
                ("v1.0", RefKind::Tag),
                ("light", RefKind::Tag),
            ]
        );
        assert!(refs[0].is_head);
        assert!(!refs[1].is_head);
        assert_eq!(refs[0].upstream.as_deref(), Some("origin/main"));
        assert_eq!(refs[2].remote.as_deref(), Some("origin"));
        assert_eq!(refs[3].target, A, "annotated tags are peeled to the commit");
    }

    #[test]
    fn resolves_remote_names_with_slashes() {
        let remotes = vec!["team".to_string(), "team/fork".to_string()];
        assert_eq!(remote_of("team/fork/main", &remotes), "team/fork");
        assert_eq!(remote_of("team/main", &remotes), "team");
        assert_eq!(remote_of("unknown/main", &remotes), "unknown");
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse_refs("refs/heads/main\u{1f}abc", &[]).is_err());
        assert!(parse_refs("", &[]).expect("empty").is_empty());
    }

    #[test]
    fn parses_head_states() {
        assert_eq!(
            parse_head("refs/heads/main\n", &format!("{A}\n")).expect("branch"),
            Head::Branch {
                name: "main".into(),
                oid: Some(A.into())
            }
        );
        assert_eq!(
            parse_head("refs/heads/main\n", "").expect("unborn"),
            Head::Branch {
                name: "main".into(),
                oid: None
            }
        );
        assert_eq!(
            parse_head("", A).expect("detached"),
            Head::Detached { oid: A.into() }
        );
        assert!(parse_head("", "").is_err());
    }

    #[test]
    fn parses_remote_list() {
        assert_eq!(
            parse_remotes("origin\nteam/fork\n\n"),
            ["origin", "team/fork"]
        );
    }
}
