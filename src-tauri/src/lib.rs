//! Code Orchestrate backend. Rust owns all system access (ADR 0001); the
//! webview reaches the system only through the commands registered here.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Follow the OS light/dark theme (spec §8.4).
            app.handle().set_theme(None);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
