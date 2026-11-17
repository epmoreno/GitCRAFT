//! Visual interactive rebase (task 13): from a commit of the current branch
//! up to HEAD, keep, reword, squash into the one below, drop and reorder.
//!
//! This module is the read-only part (13.1): what the range is, why it
//! cannot be done, and what a plan would do, predicted by replaying it in
//! objects only (`merge-tree` and `commit-tree`, as in the rebase preview of
//! task 8): loose objects are written, never refs, and git's gc removes them.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ops::branch::BlockingFile;
use crate::ops::in_progress::CommitRef;
use crate::ops::merge_rebase::{MoveBlock, in_the_way, merge_tree};
use crate::ops::reset::{remote_branches_containing, state_fingerprint};
use crate::ops::status::{in_progress, working_status};
use crate::parse::diff::fingerprint;
use crate::runner::GitVersion;
use crate::{CancellationToken, Git, GitError, Result};

/// Most commits one interactive rebase takes.
pub const MAX_COMMITS: usize = 500;

/// git that understands `fixup -C` (new messages without an editor).
pub const MESSAGES_SINCE: GitVersion = GitVersion {
    major: 2,
    minor: 32,
    patch: 0,
};

/// git whose `merge-tree --write-tree --merge-base` predicts exactly.
pub const EXACT_SINCE: GitVersion = GitVersion {
    major: 2,
    minor: 40,
    patch: 0,
};

/// Why it cannot be done at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum InteractiveRefusal {
    /// A merge, rebase… is waiting for the user.
    InProgress,
    /// HEAD is not on a branch (or there are no commits yet).
    Detached,
    /// The commit is not in the current branch.
    NotInBranch,
    /// The range has merges, which would have to be redone.
    MergesInRange,
    /// More than [`MAX_COMMITS`] commits.
    TooMany,
}

/// One commit of the range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RangeCommit {
    pub commit: CommitRef,
    /// The whole message, without the trailing newline.
    pub message: String,
    pub author: String,
    /// Seconds since the epoch.
    pub author_time: i64,
    /// A remote branch has it: rewriting it needs a forced push.
    pub published: bool,
    /// Its parent (`None`: the root commit).
    #[serde(skip)]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InteractivePreview {
    /// The current branch (`None`: detached HEAD or no commits).
    pub branch: Option<String>,
    /// The commit chosen: the oldest one of the range.
    pub from: String,
    /// Its parent, which the range is replayed onto (`None`: it is the root
    /// commit, and the whole branch is replayed).
    pub base: Option<String>,
    pub refused: Option<InteractiveRefusal>,
    /// Oldest first, as git applies them; empty when refused.
    pub commits: Vec<RangeCommit>,
    /// Remote branches with some commit of the range.
    pub published_in: Vec<String>,
    /// Uncommitted changes in the way, and which files.
    pub blocked: Option<MoveBlock>,
    pub blocking: Vec<BlockingFile>,
    /// New messages (reword and squash) are possible (git ≥ 2.32).
    pub messages: bool,
    /// Conflicts can be predicted exactly (git ≥ 2.40).
    pub exact: bool,
    /// The repository as the preview saw it: nothing is done if it changed.
    pub fingerprint: String,
}

/// What to do with one commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum StepAction {
    /// Keep it as it is.
    Pick,
    /// Keep it with a new message.
    Reword,
    /// Combine it with the commit before it (below in the list); `message`
    /// is the message of the combined commit.
    Squash,
    /// Leave it out.
    Drop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlanStep {
    pub oid: String,
    pub action: StepAction,
    /// Reword and squash: the new message.
    pub message: Option<String>,
}

/// The user's plan: every commit of the range once, in the order git will
/// apply them (oldest first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RebasePlan {
    pub from: String,
    pub steps: Vec<PlanStep>,
    /// The preview's fingerprint.
    pub expected: String,
}

/// Why a plan cannot be applied as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PlanProblem {
    /// The repository changed since the preview: make a new one.
    Changed,
    /// The preview refuses it (see [`InteractivePreview::refused`]).
    Refused,
    /// The plan does not have every commit of the range exactly once.
    Mismatch,
    /// The first commit kept is combined with nothing below it.
    SquashFirst,
    /// Every commit is dropped.
    AllDropped,
    /// A reword or squash without a message.
    EmptyMessage { oid: String },
    /// Reword and squash need git ≥ 2.32.
    MessagesNeedNewerGit,
    /// The plan leaves everything as it is.
    Unchanged,
}

/// Where the plan would stop with conflicts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlanStop {
    /// 1-based, among the commits kept (`of` of them).
    pub step: u32,
    pub of: u32,
    pub commit: CommitRef,
    pub files: Vec<String>,
}

/// What a plan would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlanPreview {
    pub problem: Option<PlanProblem>,
    /// Commits the plan drops.
    pub dropped: Vec<CommitRef>,
    /// Commits whose change is already there once reordered: git leaves
    /// them out.
    pub empty: Vec<CommitRef>,
    pub stops_at: Option<PlanStop>,
    /// `false`: this git cannot predict conflicts, so `stops_at` is unknown.
    pub predicted: bool,
    /// Commits the branch ends with, from the chosen one (if it does not
    /// stop).
    pub commits_after: u32,
    /// Commits that get new ids or are dropped.
    pub rewritten: u32,
    /// Remote branches with some of the rewritten or dropped commits.
    pub published_in: Vec<String>,
    /// The tree the branch would end with, if it does not stop and it could
    /// be predicted (for the tests; the UI does not need it).
    #[serde(skip)]
    pub tree: Option<String>,
}

impl PlanPreview {
    fn problem(problem: PlanProblem) -> Self {
        Self {
            problem: Some(problem),
            dropped: Vec::new(),
            empty: Vec::new(),
            stops_at: None,
            predicted: true,
            commits_after: 0,
            rewritten: 0,
            published_in: Vec::new(),
            tree: None,
        }
    }
}

fn split_z(stdout: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

async fn one_line(
    git: &Git,
    repo: &Path,
    args: &[&str],
    cancel: &CancellationToken,
) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(args)
        .accept_exit_code(1)
        .accept_exit_code(128)
        .output(cancel)
        .await?;
    let line = out.stdout_lossy().trim().to_owned();
    Ok((out.exit_code == Some(0) && !line.is_empty()).then_some(line))
}

/// The commit `from` (an id or a prefix, from the UI) names.
async fn resolve_commit(
    git: &Git,
    repo: &Path,
    from: &str,
    cancel: &CancellationToken,
) -> Result<String> {
    let valid = (4..=64).contains(&from.len()) && from.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(GitError::Parse(format!("not a commit id: {from:?}")));
    }
    let spec = format!("{from}^{{commit}}");
    one_line(
        git,
        repo,
        &["rev-parse", "-q", "--verify", "--end-of-options", &spec],
        cancel,
    )
    .await?
    .ok_or_else(|| GitError::Parse(format!("no such commit: {from:?}")))
}

/// The commits of `range` (`base..HEAD`, or `HEAD`), oldest first.
async fn range_commits(
    git: &Git,
    repo: &Path,
    range: &str,
    cancel: &CancellationToken,
) -> Result<Vec<RangeCommit>> {
    let out = git
        .read(repo)
        .args([
            "log",
            "-z",
            "--reverse",
            "--no-show-signature",
            "--format=%H%x1f%h%x1f%P%x1f%an%x1f%at%x1f%s%x1f%B",
            range,
            "--",
        ])
        .output(cancel)
        .await?;
    split_z(&out.stdout)
        .into_iter()
        .map(|record| {
            let record = record.trim_start_matches('\n');
            let parts: Vec<&str> = record.splitn(7, '\u{1f}').collect();
            let [oid, short, parents, author, time, subject, message] = parts[..] else {
                return Err(GitError::Parse(format!("log record: {record:?}")));
            };
            Ok(RangeCommit {
                commit: CommitRef {
                    oid: oid.to_owned(),
                    short: short.to_owned(),
                    subject: subject.to_owned(),
                },
                message: message.trim_end_matches('\n').to_owned(),
                author: author.to_owned(),
                author_time: time.parse().unwrap_or(0),
                published: false,
                parent: parents
                    .split(' ')
                    .next()
                    .filter(|p| !p.is_empty())
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// What an interactive rebase from `from` (a commit of the current branch)
/// up to HEAD would take. Read-only.
pub async fn interactive_rebase_preview(
    git: &Git,
    repo: &Path,
    from: &str,
    cancel: &CancellationToken,
) -> Result<InteractivePreview> {
    let from = resolve_commit(git, repo, from, cancel).await?;
    let version = git.version().await?;
    let branch = one_line(
        git,
        repo,
        &["symbolic-ref", "-q", "--short", "HEAD"],
        cancel,
    )
    .await?;
    let head = one_line(
        git,
        repo,
        &["rev-parse", "-q", "--verify", "HEAD^{commit}"],
        cancel,
    )
    .await?;
    let branch = head.as_ref().and(branch);
    let state = state_fingerprint(git, repo, cancel).await?;
    let mut preview = InteractivePreview {
        fingerprint: fingerprint(
            format!("{state}\0{}\0{from}", branch.as_deref().unwrap_or("")).as_bytes(),
        ),
        branch: branch.clone(),
        from: from.clone(),
        base: None,
        refused: None,
        commits: Vec::new(),
        published_in: Vec::new(),
        blocked: None,
        blocking: Vec::new(),
        messages: version >= MESSAGES_SINCE,
        exact: version >= EXACT_SINCE,
    };
    let refuse = |mut p: InteractivePreview, why| {
        p.refused = Some(why);
        Ok(p)
    };
    if in_progress(git, repo, cancel).await?.is_some() {
        return refuse(preview, InteractiveRefusal::InProgress);
    }
    if branch.is_none() {
        return refuse(preview, InteractiveRefusal::Detached);
    }
    let ancestor = git
        .read(repo)
        .args(["merge-base", "--is-ancestor", &from, "HEAD"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    if ancestor.exit_code != Some(0) {
        return refuse(preview, InteractiveRefusal::NotInBranch);
    }
    let parents = one_line(
        git,
        repo,
        &["rev-list", "--parents", "-n", "1", &from],
        cancel,
    )
    .await?
    .unwrap_or_default();
    let parents: Vec<&str> = parents.split(' ').skip(1).collect();
    if parents.len() > 1 {
        return refuse(preview, InteractiveRefusal::MergesInRange);
    }
    preview.base = parents.first().map(|p| (*p).to_owned());
    let range = match &preview.base {
        Some(base) => format!("{base}..HEAD"),
        None => "HEAD".to_owned(),
    };
    let count = one_line(git, repo, &["rev-list", "--count", &range], cancel)
        .await?
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0);
    if count > MAX_COMMITS {
        return refuse(preview, InteractiveRefusal::TooMany);
    }
    let merges = one_line(
        git,
        repo,
        &["rev-list", "--merges", "-n", "1", &range],
        cancel,
    )
    .await?;
    if merges.is_some() {
        return refuse(preview, InteractiveRefusal::MergesInRange);
    }

    let mut commits = range_commits(git, repo, &range, cancel).await?;
    let out = git
        .read(repo)
        .args(["rev-list", &range, "--not", "--remotes"])
        .output(cancel)
        .await?;
    let not_published: BTreeSet<String> = out.stdout_lossy().lines().map(str::to_owned).collect();
    for commit in &mut commits {
        commit.published = !not_published.contains(&commit.commit.oid);
    }
    if let Some(oldest) = commits.iter().find(|c| c.published) {
        preview.published_in =
            remote_branches_containing(git, repo, &oldest.commit.oid, cancel).await?;
    }

    // In the way: like git's rebase, any change in a tracked file; and
    // untracked files that a commit of the range writes.
    let status = working_status(git, repo, cancel).await?;
    let out = git
        .read(repo)
        .args([
            "log",
            "-z",
            "--format=",
            "--name-only",
            "--no-renames",
            &range,
            "--",
        ])
        .output(cancel)
        .await?;
    let touched: BTreeSet<String> = split_z(&out.stdout)
        .into_iter()
        .map(|p| p.trim_start_matches('\n').to_owned())
        .filter(|p| !p.is_empty())
        .collect();
    let dirty = !status.staged.is_empty() || !status.unstaged.is_empty();
    let files = in_the_way(&status, &touched, dirty);
    if !files.is_empty() {
        preview.blocked = Some(if dirty {
            MoveBlock::RebaseNeedsClean
        } else {
            MoveBlock::Overwrite
        });
        preview.blocking = files;
    }
    preview.commits = commits;
    Ok(preview)
}

fn is_rewording(step: &PlanStep, original: &RangeCommit) -> bool {
    step.action == StepAction::Reword
        && step.message.as_deref().map(str::trim_end) != Some(original.message.trim_end())
}

/// What is wrong with `plan` for `preview`, if anything. Pure: the checks
/// that need no git.
pub fn validate_plan(preview: &InteractivePreview, plan: &RebasePlan) -> Option<PlanProblem> {
    if preview.refused.is_some() {
        return Some(PlanProblem::Refused);
    }
    let by_oid: HashMap<&str, &RangeCommit> = preview
        .commits
        .iter()
        .map(|c| (c.commit.oid.as_str(), c))
        .collect();
    let planned: BTreeSet<&str> = plan.steps.iter().map(|s| s.oid.as_str()).collect();
    if plan.steps.len() != preview.commits.len()
        || planned.len() != plan.steps.len()
        || !planned.iter().all(|oid| by_oid.contains_key(oid))
    {
        return Some(PlanProblem::Mismatch);
    }
    let kept: Vec<&PlanStep> = plan
        .steps
        .iter()
        .filter(|s| s.action != StepAction::Drop)
        .collect();
    let Some(first) = kept.first() else {
        return Some(PlanProblem::AllDropped);
    };
    if first.action == StepAction::Squash {
        return Some(PlanProblem::SquashFirst);
    }
    let mut needs_messages = false;
    for step in &kept {
        if matches!(step.action, StepAction::Reword | StepAction::Squash) {
            if step.message.as_deref().is_none_or(|m| m.trim().is_empty()) {
                return Some(PlanProblem::EmptyMessage {
                    oid: step.oid.clone(),
                });
            }
            if step.action == StepAction::Squash || is_rewording(step, by_oid[step.oid.as_str()]) {
                needs_messages = true;
            }
        }
    }
    if needs_messages && !preview.messages {
        return Some(PlanProblem::MessagesNeedNewerGit);
    }
    let same_order = plan
        .steps
        .iter()
        .zip(&preview.commits)
        .all(|(s, c)| s.oid == c.commit.oid);
    let all_kept = plan.steps.iter().all(|s| match s.action {
        StepAction::Pick => true,
        StepAction::Reword => !is_rewording(s, by_oid[s.oid.as_str()]),
        StepAction::Squash | StepAction::Drop => false,
    });
    (same_order && all_kept).then_some(PlanProblem::Unchanged)
}

async fn tree_of(
    git: &Git,
    repo: &Path,
    commit: &str,
    cancel: &CancellationToken,
) -> Result<String> {
    let spec = format!("{commit}^{{tree}}");
    Ok(git
        .read(repo)
        .args(["rev-parse", &spec])
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned())
}

/// A throwaway commit (no ref points at it; git's gc removes it).
async fn throwaway_commit(
    git: &Git,
    repo: &Path,
    tree: &str,
    parent: Option<&str>,
    cancel: &CancellationToken,
) -> Result<String> {
    let mut args = vec!["commit-tree", "--no-gpg-sign", tree];
    if let Some(parent) = parent {
        args.extend(["-p", parent]);
    }
    args.extend(["-m", "preview"]);
    Ok(git
        .read(repo)
        .args(args)
        .env("GIT_AUTHOR_NAME", "GitCRAFT")
        .env("GIT_AUTHOR_EMAIL", "preview@gitcraft.invalid")
        .env("GIT_COMMITTER_NAME", "GitCRAFT")
        .env("GIT_COMMITTER_EMAIL", "preview@gitcraft.invalid")
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned())
}

/// The empty tree's id in this repository's hash.
async fn empty_tree(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<String> {
    Ok(git
        .read(repo)
        .args(["hash-object", "-t", "tree", "--stdin"])
        .stdin(Vec::new())
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned())
}

/// What applying `plan` would do, replayed in objects only. Read-only,
/// except for loose objects.
pub async fn simulate_plan(
    git: &Git,
    repo: &Path,
    plan: &RebasePlan,
    cancel: &CancellationToken,
) -> Result<PlanPreview> {
    let preview = interactive_rebase_preview(git, repo, &plan.from, cancel).await?;
    if preview.fingerprint != plan.expected {
        return Ok(PlanPreview::problem(PlanProblem::Changed));
    }
    if let Some(problem) = validate_plan(&preview, plan) {
        return Ok(PlanPreview::problem(problem));
    }
    let by_oid: HashMap<&str, &RangeCommit> = preview
        .commits
        .iter()
        .map(|c| (c.commit.oid.as_str(), c))
        .collect();
    let kept_total = plan
        .steps
        .iter()
        .filter(|s| s.action != StepAction::Drop)
        .count();
    let mut result = PlanPreview {
        problem: None,
        dropped: Vec::new(),
        empty: Vec::new(),
        stops_at: None,
        predicted: true,
        commits_after: 0,
        rewritten: 0,
        published_in: Vec::new(),
        tree: None,
    };
    let mut tip = preview.base.clone();
    let mut tip_tree = match &tip {
        Some(base) => Some(tree_of(git, repo, base, cancel).await?),
        None => None,
    };
    // Leading commits kept as they are: git reuses them.
    let mut reused = true;
    let mut kept_index = 0u32;
    let mut oldest_lost: Option<String> = None;
    for step in &plan.steps {
        let commit = by_oid[step.oid.as_str()];
        let mut lost = |result: &mut PlanPreview| {
            result.rewritten += 1;
            if commit.published && oldest_lost.is_none() {
                oldest_lost = Some(commit.commit.oid.clone());
            }
        };
        if step.action == StepAction::Drop {
            reused = false;
            result.dropped.push(commit.commit.clone());
            lost(&mut result);
            continue;
        }
        kept_index += 1;
        let unchanged = match step.action {
            StepAction::Pick => true,
            StepAction::Reword => !is_rewording(step, commit),
            StepAction::Squash | StepAction::Drop => false,
        };
        if reused && unchanged && tip == commit.parent {
            tip = Some(commit.commit.oid.clone());
            tip_tree = Some(tree_of(git, repo, &commit.commit.oid, cancel).await?);
            result.commits_after += 1;
            continue;
        }
        reused = false;
        lost(&mut result);
        if result.stops_at.is_some() || !result.predicted {
            if step.action != StepAction::Squash {
                result.commits_after += 1;
            }
            continue;
        }
        let tree = match (&tip, &commit.parent) {
            (None, _) => tree_of(git, repo, &commit.commit.oid, cancel).await?,
            (Some(onto), parent) => {
                let base = match parent {
                    Some(parent) => parent.clone(),
                    None => empty_tree(git, repo, cancel).await?,
                };
                let args = [
                    format!("--merge-base={base}"),
                    onto.clone(),
                    commit.commit.oid.clone(),
                ];
                match merge_tree(git, repo, &args, cancel).await {
                    Ok(Ok(tree)) => tree,
                    Ok(Err(files)) => {
                        result.stops_at = Some(PlanStop {
                            step: kept_index,
                            of: u32::try_from(kept_total).unwrap_or(u32::MAX),
                            commit: commit.commit.clone(),
                            files,
                        });
                        if step.action != StepAction::Squash {
                            result.commits_after += 1;
                        }
                        continue;
                    }
                    Err(GitError::Cancelled) => return Err(GitError::Cancelled),
                    Err(_) => {
                        // This git cannot replay in objects: unknown.
                        result.predicted = false;
                        if step.action != StepAction::Squash {
                            result.commits_after += 1;
                        }
                        continue;
                    }
                }
            }
        };
        let originally_empty = match &commit.parent {
            Some(parent) => {
                tree_of(git, repo, parent, cancel).await?
                    == tree_of(git, repo, &commit.commit.oid, cancel).await?
            }
            None => false,
        };
        if step.action != StepAction::Squash
            && !originally_empty
            && tip_tree.as_deref() == Some(tree.as_str())
        {
            // Its change is already there: git leaves it out.
            result.empty.push(commit.commit.clone());
            continue;
        }
        let new = throwaway_commit(git, repo, &tree, tip.as_deref(), cancel).await?;
        tip = Some(new);
        tip_tree = Some(tree);
        if step.action != StepAction::Squash {
            result.commits_after += 1;
        }
    }
    if result.stops_at.is_none() && result.predicted {
        result.tree = tip_tree;
    }
    if let Some(oldest) = oldest_lost {
        result.published_in = remote_branches_containing(git, repo, &oldest, cancel).await?;
    }
    Ok(result)
}
