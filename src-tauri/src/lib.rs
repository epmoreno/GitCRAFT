//! GitCRAFT desktop shell. Thin layer: Tauri commands delegate to `gitcraft_core`.

mod commands;
mod error;
mod settings;
mod state;
mod watcher;

use gitcraft_core::Git;
use std::sync::Arc;
use tauri::Manager;

use tauri_specta::{Builder, Event, collect_commands, collect_events};

use crate::settings::SettingsFile;
use crate::state::{AppState, RepoChanged};

/// Registers every command; also the source of `src/ipc/bindings.ts`.
fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .events(collect_events![RepoChanged])
        .commands(collect_commands![
            commands::open_repository,
            commands::close_repository,
            commands::list_recent_repositories,
            commands::load_graph,
            commands::get_refs,
            commands::optimize_repository,
            commands::cancel_operation,
            commands::dismiss_optimize_notice,
            commands::commit_details,
            commands::commit_changes,
            commands::commit_file_diff,
            commands::working_status,
            commands::stage_files,
            commands::unstage_files,
            commands::create_commit,
            commands::amend_info,
            commands::worktree_file_diff,
            commands::apply_selection,
            commands::discard_preview,
            commands::discard_changes,
            commands::undo_discard,
            commands::check_branch_name,
            commands::create_branch,
            commands::rename_branch,
            commands::branch_deletion_preview,
            commands::delete_branch,
            commands::checkout_branch,
            commands::stash_and_checkout,
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let settings = app.path().app_config_dir()?.join("settings.json");
            let handle = app.handle().clone();
            app.manage(AppState::new(
                Git::discover().ok(),
                SettingsFile::new(settings),
                Arc::new(move |event: RepoChanged| {
                    let _ = event.emit(&handle);
                }),
            ));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running GitCRAFT");
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use specta_typescript::Typescript;

    /// The TypeScript bindings are generated from the Rust commands and checked
    /// in. Regenerate with `npm run bindings`.
    #[test]
    fn typescript_bindings_are_up_to_date() {
        let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/ipc/bindings.ts");
        let dir = tempfile::tempdir().expect("tempdir");
        let fresh = dir.path().join("bindings.ts");
        super::specta_builder()
            .export(Typescript::default(), &fresh)
            .expect("export bindings");
        let fresh = std::fs::read_to_string(&fresh).expect("read fresh bindings");

        if std::env::var_os("UPDATE_BINDINGS").is_some() {
            std::fs::write(&committed, &fresh).expect("write bindings");
            return;
        }
        let current = std::fs::read_to_string(&committed).unwrap_or_default();
        assert!(
            current == fresh,
            "src/ipc/bindings.ts is stale: run `npm run bindings`"
        );
    }
}
