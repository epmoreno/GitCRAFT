//! Visual interactive rebase (task 13): from a commit of the current branch
//! up to HEAD, keep, reword, squash into the one below, drop and reorder.
//!
//! This module is the read-only part (13.1): what the range is, why it
//! cannot be done, and what a plan would do, predicted by replaying it in
//! objects only (`merge-tree` and `commit-tree`, as in the rebase preview of
//! task 8): loose objects are written, never refs, and git's gc removes them.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ops::branch::BlockingFile;
use crate::ops::in_progress::CommitRef;
use crate::ops::merge_rebase::{MoveBlock, in_the_way, merge_tree, stopped_or_failed};
use crate::ops::reset::{remote_branches_containing, state_fingerprint};
use crate::ops::stash::create_stash;
use crate::ops::status::{in_progress, working_status};
use crate::parse::diff::fingerprint;
use crate::runner::GitVersion;
use crate::{CancellationToken, Git, GitError, Result};

/// File the helper leaves in git's `rebase-merge` directory: the `fixup -C`
/// lines of this rebase only carry new messages, so they are not commits.
pub const MESSAGES_MARK: &str = "gitcraft-messages";

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
    #[cfg_attr(feature = "specta", specta(type = i32))]
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
    /// The git in use (`2.31.1`), to say why something is not available.
    pub git_version: String,
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
        git_version: version.to_string(),
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

// --- applying a plan (13.2) ---------------------------------------------------

/// The flag that starts the todo helper (the app's own executable as git's
/// sequence editor): `<program> --rebase-todo <plan file> <git's todo>`.
pub const TODO_HELPER_FLAG: &str = "--rebase-todo";
/// The one-time token the helper needs, set only for the `git rebase -i`
/// that GitCRAFT starts.
pub const TOKEN_ENV: &str = "GITCRAFT_REBASE_TOKEN";
/// First line of a plan file.
const PLAN_HEADER: &str = "gitcraft-rebase-plan 1";
/// The plan file, in the repository's git directory while the rebase
/// starts.
const PLAN_FILE: &str = "gitcraft-rebase-plan";

/// What the user accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InteractiveRequest {
    pub plan: RebasePlan,
    /// Stash the changes in the way first.
    pub save_changes: bool,
    /// Start even if it is predicted to stop with conflicts.
    pub allow_conflicts: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum InteractiveOutcome {
    /// Done: the branch is at `head`.
    Done { head: String, saved: Option<String> },
    /// Stopped on conflicts in these files: a rebase is waiting (task 9's
    /// banner and view).
    Conflicted {
        files: Vec<String>,
        saved: Option<String>,
    },
    /// Nothing was done; the previews say why. `changed`: the repository
    /// is no longer as the preview the user accepted saw it.
    NotDone {
        preview: Box<InteractivePreview>,
        plan: Box<PlanPreview>,
        changed: bool,
        saved: Option<String>,
    },
}

/// Message of the stash made by "save my changes, then apply".
pub fn interactive_stash_message(branch: &str) -> String {
    format!("GitCRAFT: antes del rebase interactivo de {branch}")
}

/// A token nobody outside this process can guess: std's `RandomState` is
/// seeded from the system's randomness. Not for cryptography; it only
/// makes sure the todo helper runs for the rebase GitCRAFT started.
pub fn new_token() -> String {
    use std::hash::BuildHasher;
    let now = std::time::SystemTime::now();
    (0..2)
        .map(|i| {
            let hash = std::collections::hash_map::RandomState::new().hash_one((
                i,
                std::process::id(),
                now,
            ));
            format!("{hash:016x}")
        })
        .collect()
}

/// `path` for git's shell: in single quotes (a `'` inside closes, escapes
/// and reopens), with `/` also on Windows (git's sh there reads both).
fn shell_quoted(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The `GIT_SEQUENCE_EDITOR` value: git runs it with its own shell and
/// appends the todo's path.
pub fn sequence_editor(program: &Path, plan_file: &Path) -> String {
    format!(
        "{} {TODO_HELPER_FLAG} {}",
        shell_quoted(program),
        shell_quoted(plan_file)
    )
}

/// The todo git runs for `plan`: new messages go in throwaway commits
/// (`messages`, by the oid of the last commit of their chain) applied with
/// `fixup -C`; dropped commits are listed last.
pub fn todo_text(plan: &RebasePlan, messages: &HashMap<String, String>) -> String {
    let mut lines = Vec::new();
    let mut drops = Vec::new();
    let mut chain_last: Option<&str> = None;
    let close = |lines: &mut Vec<String>, last: Option<&str>| {
        if let Some(message) = last.and_then(|oid| messages.get(oid)) {
            lines.push(format!("fixup -C {message}"));
        }
    };
    for step in &plan.steps {
        match step.action {
            StepAction::Drop => drops.push(format!("drop {}", step.oid)),
            StepAction::Pick | StepAction::Reword => {
                close(&mut lines, chain_last);
                lines.push(format!("pick {}", step.oid));
                chain_last = Some(&step.oid);
            }
            StepAction::Squash => {
                lines.push(format!("fixup {}", step.oid));
                chain_last = Some(&step.oid);
            }
        }
    }
    close(&mut lines, chain_last);
    lines.extend(drops);
    lines.push(String::new());
    lines.join("\n")
}

/// For each chain (a kept commit and the ones combined into it) that ends
/// with a new message: the oid of its last commit and that message.
fn new_messages(preview: &InteractivePreview, plan: &RebasePlan) -> Vec<(String, String)> {
    let by_oid: HashMap<&str, &RangeCommit> = preview
        .commits
        .iter()
        .map(|c| (c.commit.oid.as_str(), c))
        .collect();
    let mut chains: Vec<(String, Option<String>)> = Vec::new();
    for step in &plan.steps {
        match step.action {
            StepAction::Drop => {}
            StepAction::Pick => chains.push((step.oid.clone(), None)),
            StepAction::Reword => {
                let message = is_rewording(step, by_oid[step.oid.as_str()])
                    .then(|| step.message.clone())
                    .flatten();
                chains.push((step.oid.clone(), message));
            }
            StepAction::Squash => {
                if let Some(chain) = chains.last_mut() {
                    *chain = (step.oid.clone(), step.message.clone());
                }
            }
        }
    }
    chains
        .into_iter()
        .filter_map(|(oid, message)| message.map(|m| (oid, m)))
        .collect()
}

/// A commit with `message` and no change of its own on top of `oid`: with
/// `fixup -C`, git takes only its message. No ref points at it.
async fn message_commit(
    git: &Git,
    repo: &Path,
    oid: &str,
    message: &str,
    cancel: &CancellationToken,
) -> Result<String> {
    let tree = tree_of(git, repo, oid, cancel).await?;
    let mut text = message.trim_end().to_owned();
    text.push('\n');
    Ok(git
        .read(repo)
        .args(["commit-tree", "--no-gpg-sign", &tree, "-p", oid, "-F", "-"])
        .stdin(text.into_bytes())
        .env("GIT_AUTHOR_NAME", "GitCRAFT")
        .env("GIT_AUTHOR_EMAIL", "rebase@gitcraft.invalid")
        .env("GIT_COMMITTER_NAME", "GitCRAFT")
        .env("GIT_COMMITTER_EMAIL", "rebase@gitcraft.invalid")
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned())
}

/// Applies the plan the user accepted with `git rebase -i`, with `editor`
/// (the app's executable) as the sequence editor. It checks everything
/// again and does nothing if the repository changed since the preview, if
/// it is refused or the plan is wrong, if changes are in the way (unless
/// `save_changes`: they are stashed first) or if it would stop with
/// conflicts (unless `allow_conflicts`).
pub async fn interactive_rebase(
    git: &Git,
    repo: &Path,
    request: &InteractiveRequest,
    editor: &Path,
    cancel: &CancellationToken,
) -> Result<InteractiveOutcome> {
    let not_done = |preview: InteractivePreview, plan: PlanPreview, changed, saved| {
        Ok(InteractiveOutcome::NotDone {
            preview: Box::new(preview),
            plan: Box::new(plan),
            changed,
            saved,
        })
    };
    let plan = &request.plan;
    let mut preview = interactive_rebase_preview(git, repo, &plan.from, cancel).await?;
    if preview.fingerprint != plan.expected {
        return not_done(
            preview,
            PlanPreview::problem(PlanProblem::Changed),
            true,
            None,
        );
    }
    if let Some(problem) = validate_plan(&preview, plan) {
        return not_done(preview, PlanPreview::problem(problem), false, None);
    }
    let mut saved = None;
    if preview.blocked.is_some() {
        // Nothing wrong with the plan: the preview says what is in the way.
        let mut in_the_way = PlanPreview::problem(PlanProblem::Refused);
        in_the_way.problem = None;
        if !request.save_changes {
            return not_done(preview, in_the_way, false, None);
        }
        let branch = preview.branch.clone().unwrap_or_default();
        let message = interactive_stash_message(&branch);
        saved = Some(create_stash(git, repo, &message, true, cancel).await?);
        preview = interactive_rebase_preview(git, repo, &plan.from, cancel).await?;
        if preview.refused.is_some() || preview.blocked.is_some() {
            return not_done(preview, in_the_way, false, saved);
        }
    }
    // The stash changed the fingerprint: the plan is checked against the
    // preview made now.
    let current = RebasePlan {
        expected: preview.fingerprint.clone(),
        ..plan.clone()
    };
    let predicted = simulate_plan(git, repo, &current, cancel).await?;
    if predicted.problem.is_some() || (predicted.stops_at.is_some() && !request.allow_conflicts) {
        return not_done(preview, predicted, false, saved);
    }

    let mut messages = HashMap::new();
    for (oid, message) in new_messages(&preview, &current) {
        let commit = message_commit(git, repo, &oid, &message, cancel).await?;
        messages.insert(oid, commit);
    }
    let todo = todo_text(&current, &messages);
    let git_dir = PathBuf::from(
        git.read(repo)
            .args(["rev-parse", "--absolute-git-dir"])
            .output(cancel)
            .await?
            .stdout_lossy()
            .trim(),
    );
    let plan_file = git_dir.join(PLAN_FILE);
    let token = new_token();
    let expected: Vec<&str> = preview
        .commits
        .iter()
        .map(|c| c.commit.oid.as_str())
        .collect();
    let contents = format!("{PLAN_HEADER}\n{token}\n{}\n{todo}", expected.join(" "));
    // A plan file left by a rebase that never started is stale.
    let _ = std::fs::remove_file(&plan_file);
    write_new(&plan_file, &contents)?;

    let mut cmd = git
        .write(repo)
        .args([
            // `-c`, not options: older git ignores unknown settings.
            "-c",
            "rebase.updateRefs=false",
            "-c",
            "rebase.autoSquash=false",
            "-c",
            "rebase.rebaseMerges=false",
            "-c",
            "rebase.abbreviateCommands=false",
            "-c",
            "rebase.missingCommitsCheck=ignore",
            "-c",
            "core.commentChar=#",
            "rebase",
            "--interactive",
            "--no-autostash",
            "--empty=drop",
        ])
        .env("GIT_SEQUENCE_EDITOR", sequence_editor(editor, &plan_file))
        .env("GIT_EDITOR", "true")
        .env(TOKEN_ENV, &token);
    cmd = match &preview.base {
        Some(base) => cmd.args(["--end-of-options", base]),
        None => cmd.arg("--root"),
    };
    let out = cmd.accept_exit_code(1).output(cancel).await;
    let _ = std::fs::remove_file(&plan_file);
    let out = out?;
    let status = working_status(git, repo, cancel).await?;
    stopped_or_failed(&out, &status, "rebase")?;
    if out.exit_code == Some(1) || !status.conflicted.is_empty() || status.in_progress.is_some() {
        return Ok(InteractiveOutcome::Conflicted {
            files: status.conflicted.into_iter().map(|c| c.path).collect(),
            saved,
        });
    }
    let head = git
        .read(repo)
        .args(["rev-parse", "HEAD"])
        .output(cancel)
        .await?
        .stdout_lossy()
        .trim()
        .to_owned();
    Ok(InteractiveOutcome::Done { head, saved })
}

/// Writes a file that must not exist yet.
fn write_new(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(contents.as_bytes()).map_err(GitError::from)
}

/// Why the todo helper did nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoHelperError {
    /// Not started by GitCRAFT's rebase: no token, or not this one.
    NotOurs,
    /// The arguments or a file could not be read.
    Unreadable(String),
    /// git proposed other commits than the preview showed.
    Mismatch,
}

impl std::fmt::Display for TodoHelperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOurs => write!(f, "only GitCRAFT's own interactive rebase can use this"),
            Self::Unreadable(why) => write!(f, "could not read the rebase plan: {why}"),
            Self::Mismatch => write!(
                f,
                "git proposed other commits than GitCRAFT's preview; nothing was changed"
            ),
        }
    }
}

/// The commit ids of git's todo, if it only picks (in order).
fn picked(todo: &str) -> Option<Vec<&str>> {
    let mut oids = Vec::new();
    for line in todo.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        if words.next()? != "pick" {
            return None;
        }
        let oid = words.next()?;
        if oid.len() < 4 || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        oids.push(oid);
    }
    Some(oids)
}

/// The todo helper: checks the token and that git's todo lists exactly the
/// commits of the preview, then puts the plan in its place. The plan file
/// is used once: it is removed before anything else is written.
pub fn apply_todo(
    plan_file: &Path,
    todo_file: &Path,
    token: Option<&str>,
) -> std::result::Result<(), TodoHelperError> {
    let unreadable = |e: std::io::Error| TodoHelperError::Unreadable(e.to_string());
    let token = token
        .filter(|t| !t.is_empty())
        .ok_or(TodoHelperError::NotOurs)?;
    let contents = std::fs::read_to_string(plan_file).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => TodoHelperError::NotOurs,
        _ => unreadable(e),
    })?;
    let mut parts = contents.splitn(4, '\n');
    let (Some(PLAN_HEADER), Some(expected_token), Some(expected), Some(todo)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(TodoHelperError::NotOurs);
    };
    if expected_token != token {
        return Err(TodoHelperError::NotOurs);
    }
    std::fs::remove_file(plan_file).map_err(unreadable)?;
    let proposed = std::fs::read_to_string(todo_file).map_err(unreadable)?;
    let expected: Vec<&str> = expected.split(' ').filter(|s| !s.is_empty()).collect();
    let matches = picked(&proposed).is_some_and(|oids| {
        oids.len() == expected.len()
            && oids
                .iter()
                .zip(&expected)
                .all(|(short, full)| full.starts_with(short))
    });
    if !matches {
        return Err(TodoHelperError::Mismatch);
    }
    std::fs::write(todo_file, todo).map_err(unreadable)?;
    // Tells the progress (task 9's notice) that the `fixup -C` lines only
    // carry messages. git removes the whole directory when the rebase ends;
    // without the mark the progress counts todo lines, as before.
    if let Some(dir) = todo_file.parent() {
        let _ = std::fs::write(dir.join(MESSAGES_MARK), "");
    }
    Ok(())
}

/// The helper's command line (`--rebase-todo <plan file> <todo>`, after the
/// program's name). `None`: these are not its arguments, the program goes
/// on as usual. Otherwise the exit code: 0 if the plan is in place.
pub fn todo_helper_main(args: &[std::ffi::OsString]) -> Option<i32> {
    if args.first().and_then(|a| a.to_str()) != Some(TODO_HELPER_FLAG) {
        return None;
    }
    let token = std::env::var(TOKEN_ENV).ok();
    let result = match args {
        [_, plan, todo] => apply_todo(Path::new(plan), Path::new(todo), token.as_deref()),
        _ => Err(TodoHelperError::Unreadable(
            "expected a plan and a todo".into(),
        )),
    };
    Some(match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("GitCRAFT: {err}");
            1
        }
    })
}
