//! Tauri commands. Thin wrappers: the logic lives in `AppState` and `gitcraft_core`.

use std::path::PathBuf;

use tauri::State;
use tauri::ipc::Channel;

use gitcraft_core::ops::branch::{BranchDeletion, BranchNameCheck, CheckoutOutcome};
use gitcraft_core::ops::commit::CommitChanges;
use gitcraft_core::ops::commit_write::AmendInfo;
use gitcraft_core::ops::discard::{DiscardTarget, DiscardedFile};
use gitcraft_core::ops::refs::RepoRefs;
use gitcraft_core::ops::worktree::{SelectionAction, WorktreeArea};
use gitcraft_core::parse::commit::CommitDetails;
use gitcraft_core::parse::diff::FileDiff;
use gitcraft_core::parse::status::WorkingStatus;
use gitcraft_core::patch::HunkSelection;

use crate::error::IpcError;
use crate::settings::RecentRepository;
use crate::state::{AppState, DiscardResult, GraphEvent, OpenedRepository, RepoId};

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
