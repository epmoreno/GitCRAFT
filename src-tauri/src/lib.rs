//! GitCRAFT desktop shell. Thin layer: Tauri commands delegate to `gitcraft_core`.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("error while running GitCRAFT");
}
