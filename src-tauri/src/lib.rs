//! Code Orchestrate backend. Rust owns all system access (ADR 0001); the
//! webview reaches the system only through the commands registered here.

pub mod agents;
pub mod commands;
pub mod launch;
pub mod paths;
pub mod store;
pub mod workspaces;

use std::sync::{Arc, Mutex};

use tauri::Manager;

/// The SQLite file's name inside the app data folder (spec §4.1).
pub const DB_FILE_NAME: &str = "code-orchestrate.db";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Follow the OS light/dark theme (spec §8.4).
            app.handle().set_theme(None);

            // %APPDATA%\com.mirispence.code-orchestrate\ on Windows.
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = store::Store::open(&data_dir.join(DB_FILE_NAME))?;
            app.manage::<commands::AppStore>(Arc::new(Mutex::new(store)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_workspaces,
            commands::add_workspace,
            commands::agent_status,
            commands::launch_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
