//! Code Orchestrate backend. Rust owns all system access (ADR 0001); the
//! webview reaches the system only through the commands registered here.

pub mod agents;
pub mod attribution;
pub mod commands;
pub mod launch;
pub mod paths;
pub mod scan;
pub mod settings;
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
            // %APPDATA%\com.mirispence.code-orchestrate\ on Windows.
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            // The theme follows the OS unless settings.json overrides it (spec §8.4).
            let settings = settings::SettingsFile::new(data_dir.join(settings::SETTINGS_FILE_NAME));
            commands::apply_settings(app.handle(), &settings.load());
            app.manage::<commands::AppSettings>(Arc::new(Mutex::new(settings)));
            let store: commands::AppStore = Arc::new(Mutex::new(store::Store::open(&data_dir.join(DB_FILE_NAME))?));
            app.manage::<commands::AppStore>(Arc::clone(&store));
            // Scan transcripts in the background on startup (spec §5.1).
            let scanner = commands::scanner(app.handle().clone(), store);
            scanner.trigger();
            app.manage(scanner);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_workspaces,
            commands::add_workspace,
            commands::repoint_workspace,
            commands::remove_former_path,
            commands::update_workspace,
            commands::set_workspace_pinned,
            commands::remove_workspace,
            commands::get_settings,
            commands::set_setting,
            commands::reset_settings,
            commands::agent_status,
            commands::launch_session,
            commands::start_scan,
            commands::scan_running,
            commands::list_suggestions,
            commands::dismiss_path,
            commands::test_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
