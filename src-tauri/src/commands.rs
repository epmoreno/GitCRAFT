//! Tauri commands. Thin wrappers: the logic lives in `AppState` and `gitcraft_core`.

use std::path::PathBuf;

use tauri::State;
use tauri::ipc::Channel;

use gitcraft_core::ops::refs::RepoRefs;
use gitcraft_core::parse::commit::CommitDetails;

use crate::error::IpcError;
use crate::settings::RecentRepository;
use crate::state::{AppState, GraphEvent, OpenedRepository, RepoId};

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
