//! Tauri commands. Thin wrappers: the logic lives in `AppState` and `gitcraft_core`.

use std::path::PathBuf;

use tauri::State;
use tauri::ipc::Channel;

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
