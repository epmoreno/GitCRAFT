//! Tauri commands. Thin wrappers: the logic lives in `AppState` and `gitcraft_core`.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;

use gitcraft_core::ops::branch::{BranchDeletion, BranchNameCheck, CheckoutOutcome};
use gitcraft_core::ops::clone::{self, Destination};
use gitcraft_core::ops::commit::CommitChanges;
use gitcraft_core::ops::commit_write::AmendInfo;
use gitcraft_core::ops::config::{self, ConfigChange, GlobalDefaults, PullStrategy};
use gitcraft_core::ops::discard::{DiscardTarget, DiscardedFile};
use gitcraft_core::ops::init::InitCheck;
use gitcraft_core::ops::refs::RepoRefs;
use gitcraft_core::ops::remote::{ForcePushPreview, PullMode, PullOutcome, PushOutcome};
use gitcraft_core::ops::stash::{ApplyOutcome, RecoverPreview};
use gitcraft_core::ops::worktree::{SelectionAction, WorktreeArea};
use gitcraft_core::parse::commit::CommitDetails;
use gitcraft_core::parse::diff::FileDiff;
use gitcraft_core::parse::stash::StashEntry;
use gitcraft_core::parse::status::WorkingStatus;
use gitcraft_core::patch::HunkSelection;

use crate::error::IpcError;
use crate::history::{CommandHistory, CommandRecord};
use crate::settings::{GeneralSettings, RecentRepository};
use crate::state::{
    AppState, DiscardResult, GraphEvent, OpenedRepository, PullModeInfo, RepoId, RestoredSession,
};

#[tauri::command]
#[specta::specta]
pub async fn open_repository(
    state: State<'_, AppState>,
    path: String,
) -> Result<OpenedRepository, IpcError> {
    state.open_repository(&PathBuf::from(path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn close_repository(state: State<'_, AppState>, id: RepoId) -> Result<bool, IpcError> {
    Ok(state.close_repository(id))
}

#[tauri::command]
#[specta::specta]
pub async fn list_recent_repositories(
    state: State<'_, AppState>,
) -> Result<Vec<RecentRepository>, IpcError> {
    Ok(state.recent_repositories())
}

/// Writes the repository's commit-graph (opt-in), streaming git's progress.
#[tauri::command]
#[specta::specta]
pub async fn optimize_repository(
    state: State<'_, AppState>,
    id: RepoId,
    on_progress: Channel<String>,
) -> Result<(), IpcError> {
    state
        .optimize_repository(id, |line| {
            let _ = on_progress.send(line.to_owned());
        })
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_operation(state: State<'_, AppState>, id: RepoId) -> Result<(), IpcError> {
    state.cancel_operation(id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn dismiss_optimize_notice(
    state: State<'_, AppState>,
    id: RepoId,
) -> Result<(), IpcError> {
    state.dismiss_optimize_notice(id)
}

/// Streams the commit graph as `page` events followed by `done`.
#[tauri::command]
#[specta::specta]
pub async fn load_graph(
    state: State<'_, AppState>,
    id: RepoId,
    on_event: Channel<GraphEvent>,
) -> Result<(), IpcError> {
    state
        .load_graph(id, |event| {
            // The window may be gone; there is nobody left to tell.
            let _ = on_event.send(event);
        })
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn get_refs(state: State<'_, AppState>, id: RepoId) -> Result<RepoRefs, IpcError> {
    state.refs(id).await
}

#[tauri::command]
#[specta::specta]
pub async fn commit_details(
    state: State<'_, AppState>,
    id: RepoId,
    oid: String,
) -> Result<CommitDetails, IpcError> {
    state.commit_details(id, &oid).await
}

#[tauri::command]
#[specta::specta]
pub async fn commit_changes(
    state: State<'_, AppState>,
    id: RepoId,
    oid: String,
) -> Result<CommitChanges, IpcError> {
    state.commit_changes(id, &oid).await
}

#[tauri::command]
#[specta::specta]
pub async fn commit_file_diff(
    state: State<'_, AppState>,
    id: RepoId,
    oid: String,
    path: String,
    old_path: Option<String>,
    full: bool,
) -> Result<FileDiff, IpcError> {
    state
        .commit_file_diff(id, &oid, &path, old_path.as_deref(), full)
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn working_status(
    state: State<'_, AppState>,
    id: RepoId,
) -> Result<WorkingStatus, IpcError> {
    state.working_status(id).await
}

/// Stages files; `paths: null` stages every change.
#[tauri::command]
#[specta::specta]
pub async fn stage_files(
    state: State<'_, AppState>,
    id: RepoId,
    paths: Option<Vec<String>>,
) -> Result<(), IpcError> {
    state.stage(id, paths).await
}

/// Unstages files; `paths: null` unstages everything. Never touches the
/// working tree.
#[tauri::command]
#[specta::specta]
pub async fn unstage_files(
    state: State<'_, AppState>,
    id: RepoId,
    paths: Option<Vec<String>>,
) -> Result<(), IpcError> {
    state.unstage(id, paths).await
}

/// Commits the staged changes (or amends the last commit) with a summary and
/// an optional description. Returns the new commit's id.
#[tauri::command]
#[specta::specta]
pub async fn create_commit(
    state: State<'_, AppState>,
    id: RepoId,
    summary: String,
    description: String,
    amend: bool,
) -> Result<String, IpcError> {
    state.commit(id, &summary, &description, amend).await
}

/// The last commit, to prefill and confirm an amend (`null` before the
/// first commit).
#[tauri::command]
#[specta::specta]
pub async fn amend_info(
    state: State<'_, AppState>,
    id: RepoId,
) -> Result<Option<AmendInfo>, IpcError> {
    state.amend_info(id).await
}

/// Diff of one file's uncommitted change (`area`: unstaged, staged or
/// untracked). Large diffs only when `full`.
#[tauri::command]
#[specta::specta]
pub async fn worktree_file_diff(
    state: State<'_, AppState>,
    id: RepoId,
    path: String,
    old_path: Option<String>,
    area: WorktreeArea,
    full: bool,
) -> Result<FileDiff, IpcError> {
    state
        .worktree_file_diff(id, &path, old_path.as_deref(), area, full)
        .await
}

/// Stages or unstages selected hunks/lines of one file. `fingerprint` is the
/// one of the diff shown; a changed file is reported as `staleDiff`.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn apply_selection(
    state: State<'_, AppState>,
    id: RepoId,
    path: String,
    old_path: Option<String>,
    area: WorktreeArea,
    fingerprint: String,
    selection: Vec<HunkSelection>,
    action: SelectionAction,
) -> Result<(), IpcError> {
    state
        .apply_selection(id, path, old_path, area, fingerprint, selection, action)
        .await
}

/// What discarding `target` would lose, to show before confirming.
#[tauri::command]
#[specta::specta]
pub async fn discard_preview(
    state: State<'_, AppState>,
    id: RepoId,
    target: DiscardTarget,
) -> Result<Vec<DiscardedFile>, IpcError> {
    state.discard_preview(id, target).await
}

/// Discards unstaged changes (tracked files are backed up first; untracked
/// files go to the Trash). Returns an id to undo it.
#[tauri::command]
#[specta::specta]
pub async fn discard_changes(
    state: State<'_, AppState>,
    id: RepoId,
    target: DiscardTarget,
) -> Result<DiscardResult, IpcError> {
    state.discard(id, target).await
}

/// Puts back what a discard removed, if the files did not change since.
#[tauri::command]
#[specta::specta]
pub async fn undo_discard(
    state: State<'_, AppState>,
    id: RepoId,
    undo_id: u32,
) -> Result<(), IpcError> {
    state.undo_discard(id, undo_id).await
}

/// Whether `name` can be used for a new branch.
#[tauri::command]
#[specta::specta]
pub async fn check_branch_name(
    state: State<'_, AppState>,
    id: RepoId,
    name: String,
) -> Result<BranchNameCheck, IpcError> {
    state.check_branch_name(id, &name).await
}

/// Creates branch `name` at `start` (a commit id or a full ref name);
/// `track` makes `start`, a remote branch, its upstream. With `checkout`
/// it then switches to it and returns the outcome.
#[tauri::command]
#[specta::specta]
pub async fn create_branch(
    state: State<'_, AppState>,
    id: RepoId,
    name: String,
    start: String,
    track: bool,
    checkout: bool,
) -> Result<Option<CheckoutOutcome>, IpcError> {
    state.create_branch(id, name, start, track, checkout).await
}

#[tauri::command]
#[specta::specta]
pub async fn rename_branch(
    state: State<'_, AppState>,
    id: RepoId,
    old_name: String,
    new_name: String,
) -> Result<(), IpcError> {
    state.rename_branch(id, old_name, new_name).await
}

/// What deleting a branch would leave without a branch, to confirm first.
#[tauri::command]
#[specta::specta]
pub async fn branch_deletion_preview(
    state: State<'_, AppState>,
    id: RepoId,
    name: String,
) -> Result<BranchDeletion, IpcError> {
    state.branch_deletion_preview(id, &name).await
}

/// Deletes a branch if it still points to `expected_tip` (the confirmed one).
#[tauri::command]
#[specta::specta]
pub async fn delete_branch(
    state: State<'_, AppState>,
    id: RepoId,
    name: String,
    expected_tip: String,
) -> Result<(), IpcError> {
    state.delete_branch(id, name, expected_tip).await
}

/// Switches to a local branch. Never forces: blocking changes are reported.
#[tauri::command]
#[specta::specta]
pub async fn checkout_branch(
    state: State<'_, AppState>,
    id: RepoId,
    name: String,
) -> Result<CheckoutOutcome, IpcError> {
    state.checkout_branch(id, name).await
}

/// Saves every uncommitted change in a stash entry, then switches.
#[tauri::command]
#[specta::specta]
pub async fn stash_and_checkout(
    state: State<'_, AppState>,
    id: RepoId,
    name: String,
    include_untracked: bool,
) -> Result<CheckoutOutcome, IpcError> {
    state.stash_and_checkout(id, name, include_untracked).await
}

/// Every stash entry, newest first.
#[tauri::command]
#[specta::specta]
pub async fn list_stashes(
    state: State<'_, AppState>,
    id: RepoId,
) -> Result<Vec<StashEntry>, IpcError> {
    state.stashes(id).await
}

/// Saves the uncommitted changes (optionally with untracked files) in a new
/// stash entry; `message` may be empty. Returns the entry's id.
#[tauri::command]
#[specta::specta]
pub async fn create_stash(
    state: State<'_, AppState>,
    id: RepoId,
    message: String,
    include_untracked: bool,
) -> Result<String, IpcError> {
    state.create_stash(id, message, include_untracked).await
}

/// Applies (`pop`: and then drops, only if clean) stash entry `oid`. Never
/// forces: blocking changes and predicted conflicts are reported unless
/// `allow_conflicts`. With `save_local` the current changes are stashed
/// first.
#[tauri::command]
#[specta::specta]
pub async fn apply_stash(
    state: State<'_, AppState>,
    id: RepoId,
    oid: String,
    pop: bool,
    allow_conflicts: bool,
    save_local: bool,
) -> Result<ApplyOutcome, IpcError> {
    state
        .apply_stash(id, oid, pop, allow_conflicts, save_local)
        .await
}

/// Deletes stash entry `oid` (the UI confirms first).
#[tauri::command]
#[specta::specta]
pub async fn drop_stash(
    state: State<'_, AppState>,
    id: RepoId,
    oid: String,
) -> Result<(), IpcError> {
    state.drop_stash(id, oid).await
}

/// What recovering one file from a stash entry would overwrite.
#[tauri::command]
#[specta::specta]
pub async fn recover_preview(
    state: State<'_, AppState>,
    id: RepoId,
    oid: String,
    path: String,
) -> Result<RecoverPreview, IpcError> {
    state.recover_preview(id, &oid, &path).await
}

/// Puts a file's version from a stash entry back in the working tree
/// (`git restore --source=<oid> --worktree`), without touching the index.
/// `current` is the preview's: a file changed since is `staleDiff`.
#[tauri::command]
#[specta::specta]
pub async fn recover_file(
    state: State<'_, AppState>,
    id: RepoId,
    oid: String,
    path: String,
    current: Option<String>,
) -> Result<(), IpcError> {
    state.recover_file(id, oid, path, current).await
}

/// Fetches from `remote` (every remote when `null`), with `--prune` when
/// asked, streaming git's progress lines. Cancel with `cancel_operation`.
#[tauri::command]
#[specta::specta]
pub async fn fetch_remote(
    state: State<'_, AppState>,
    id: RepoId,
    remote: Option<String>,
    prune: bool,
    on_progress: Channel<String>,
) -> Result<(), IpcError> {
    state
        .fetch(id, remote, prune, |line| {
            let _ = on_progress.send(line.to_owned());
        })
        .await
}

/// The pull mode in effect (chosen here, else git's configuration).
#[tauri::command]
#[specta::specta]
pub async fn get_pull_mode(
    state: State<'_, AppState>,
    id: RepoId,
) -> Result<PullModeInfo, IpcError> {
    state.pull_mode(id).await
}

/// Chooses merge or rebase for this repository's pulls; `null` follows
/// git's configuration again.
#[tauri::command]
#[specta::specta]
pub async fn set_pull_mode(
    state: State<'_, AppState>,
    id: RepoId,
    mode: Option<PullMode>,
) -> Result<(), IpcError> {
    state.set_pull_mode(id, mode)
}

/// Pulls the current branch. Never forces: uncommitted changes in the way
/// and predicted conflicts are reported first (unless `allow_conflicts`);
/// `save_local` stashes uncommitted changes before pulling.
#[tauri::command]
#[specta::specta]
pub async fn pull_branch(
    state: State<'_, AppState>,
    id: RepoId,
    allow_conflicts: bool,
    save_local: bool,
    on_progress: Channel<String>,
) -> Result<PullOutcome, IpcError> {
    state
        .pull(id, allow_conflicts, save_local, |line| {
            let _ = on_progress.send(line.to_owned());
        })
        .await
}

/// Aborts the merge/rebase/cherry-pick/revert waiting on conflicts.
#[tauri::command]
#[specta::specta]
pub async fn abort_in_progress(state: State<'_, AppState>, id: RepoId) -> Result<(), IpcError> {
    state.abort_in_progress(id).await
}

/// Pushes the current branch to its upstream, or publishes it to
/// `publish_to` creating the upstream. `force_lease`: force only if the
/// remote branch still points there (`--force-with-lease`).
#[tauri::command]
#[specta::specta]
pub async fn push_branch(
    state: State<'_, AppState>,
    id: RepoId,
    publish_to: Option<String>,
    force_lease: Option<String>,
    on_progress: Channel<String>,
) -> Result<PushOutcome, IpcError> {
    state
        .push(id, publish_to, force_lease, |line| {
            let _ = on_progress.send(line.to_owned());
        })
        .await
}

/// Fetches the upstream, then lists the remote commits a forced push would
/// overwrite (and the lease to use).
#[tauri::command]
#[specta::specta]
pub async fn force_push_preview(
    state: State<'_, AppState>,
    id: RepoId,
    on_progress: Channel<String>,
) -> Result<ForcePushPreview, IpcError> {
    state
        .force_push_preview(id, |line| {
            let _ = on_progress.send(line.to_owned());
        })
        .await
}

/// Remembers the open tabs (repository paths, in order) and the active one.
#[tauri::command]
#[specta::specta]
pub async fn save_session(
    state: State<'_, AppState>,
    tabs: Vec<String>,
    active: Option<String>,
) -> Result<(), IpcError> {
    state.save_session(tabs, active);
    Ok(())
}

/// Reopens the previous session's tabs; invalid repositories are reported
/// and forgotten.
#[tauri::command]
#[specta::specta]
pub async fn restore_session(state: State<'_, AppState>) -> Result<RestoredSession, IpcError> {
    Ok(state.restore_session().await)
}

/// What `git init` would do in `path` (shown before doing it).
#[tauri::command]
#[specta::specta]
pub async fn check_init(state: State<'_, AppState>, path: String) -> Result<InitCheck, IpcError> {
    state.check_init(&PathBuf::from(path)).await
}

/// Whether `name` can be the first branch of a new repository.
#[tauri::command]
#[specta::specta]
pub async fn valid_branch_name(
    state: State<'_, AppState>,
    path: String,
    name: String,
) -> Result<bool, IpcError> {
    state.valid_branch_name(&PathBuf::from(path), &name).await
}

/// Creates a repository in `path` with first branch `branch` and opens it.
#[tauri::command]
#[specta::specta]
pub async fn init_repository(
    state: State<'_, AppState>,
    path: String,
    branch: String,
) -> Result<OpenedRepository, IpcError> {
    state.init_repository(&PathBuf::from(path), &branch).await
}

/// Folder suggested for a new clone (the last one used, else home).
#[tauri::command]
#[specta::specta]
pub async fn clone_parent(state: State<'_, AppState>) -> Result<String, IpcError> {
    Ok(state.clone_parent())
}

/// Settings → General.
#[tauri::command]
#[specta::specta]
pub async fn general_settings(state: State<'_, AppState>) -> Result<GeneralSettings, IpcError> {
    Ok(state.general_settings())
}

#[tauri::command]
#[specta::specta]
pub async fn set_general_settings(
    state: State<'_, AppState>,
    general: GeneralSettings,
) -> Result<(), IpcError> {
    state.set_general_settings(general);
    Ok(())
}

/// Git's global defaults: `init.defaultBranch` and the pull strategy.
#[tauri::command]
#[specta::specta]
pub async fn git_defaults(state: State<'_, AppState>) -> Result<GlobalDefaults, IpcError> {
    state.git_defaults().await
}

/// What setting the default pull strategy writes (shown before doing it).
#[tauri::command]
#[specta::specta]
pub async fn pull_strategy_changes(
    strategy: Option<PullStrategy>,
) -> Result<Vec<ConfigChange>, IpcError> {
    Ok(config::pull_strategy_changes(strategy))
}

/// Writes `init.defaultBranch` to git's global config (`None` removes it).
#[tauri::command]
#[specta::specta]
pub async fn set_default_branch(
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<GlobalDefaults, IpcError> {
    state.set_default_branch(name).await
}

/// Writes the default pull strategy to git's global config.
#[tauri::command]
#[specta::specta]
pub async fn set_default_pull_strategy(
    state: State<'_, AppState>,
    strategy: Option<PullStrategy>,
) -> Result<GlobalDefaults, IpcError> {
    state.set_default_pull_strategy(strategy).await
}

/// The folder name a clone of `url` gets by default.
#[tauri::command]
#[specta::specta]
pub async fn default_clone_name(url: String) -> Result<String, IpcError> {
    Ok(clone::default_folder_name(&url))
}

/// Whether a clone can go to `parent/name` (never a folder with files).
#[tauri::command]
#[specta::specta]
pub async fn check_clone_destination(
    parent: String,
    name: String,
) -> Result<Destination, IpcError> {
    Ok(clone::check_destination(&PathBuf::from(parent), &name))
}

/// Clones `url` into `parent/name`, streaming git's progress, and opens it.
#[tauri::command]
#[specta::specta]
pub async fn clone_repository(
    state: State<'_, AppState>,
    url: String,
    parent: String,
    name: String,
    on_progress: Channel<String>,
) -> Result<OpenedRepository, IpcError> {
    state
        .clone_repository(&url, &PathBuf::from(parent), &name, |line| {
            let _ = on_progress.send(line.to_owned());
        })
        .await
}

/// Cancels the clone in progress.
#[tauri::command]
#[specta::specta]
pub async fn cancel_clone(state: State<'_, AppState>) -> Result<(), IpcError> {
    state.cancel_clone();
    Ok(())
}

/// The git commands run so far (oldest first), for the operations panel.
/// New and finished commands arrive as `CommandRecord` events.
#[tauri::command]
#[specta::specta]
pub async fn command_history(
    history: State<'_, Arc<CommandHistory>>,
) -> Result<Vec<CommandRecord>, IpcError> {
    Ok(history.records())
}

/// Forgets the finished commands of the history.
#[tauri::command]
#[specta::specta]
pub async fn clear_command_history(
    history: State<'_, Arc<CommandHistory>>,
) -> Result<(), IpcError> {
    history.clear();
    Ok(())
}
