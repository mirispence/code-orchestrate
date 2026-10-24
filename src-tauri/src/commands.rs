//! The Tauri command surface the UI calls. Commands stay thin: they hand the
//! shared store to the domain modules on a blocking worker thread, so slow
//! filesystem or SQLite work never runs on (and freezes) the main thread.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, State};

use crate::settings::{SettingsError, SettingsFile, Snapshot, Theme};
use crate::store::{Store, Workspace};
use crate::workspaces::{self, AddWorkspaceError};

/// The app's single store, shared across commands and worker threads.
pub type AppStore = Arc<Mutex<Store>>;

/// A command error for the UI: `{ kind: "internal", message }`.
#[derive(Debug, serde::Serialize)]
pub struct InternalError {
    kind: &'static str,
    message: String,
}

impl InternalError {
    fn new(message: impl ToString) -> Self {
        InternalError { kind: "internal", message: message.to_string() }
    }
}

/// Run `work` with the locked store on a blocking worker thread.
async fn with_store<T, E>(
    store: &AppStore,
    internal: impl FnOnce(String) -> E,
    work: impl FnOnce(&Store) -> Result<T, E> + Send + 'static,
) -> Result<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
{
    let store = Arc::clone(store);
    let joined = tauri::async_runtime::spawn_blocking(move || {
        let guard = store.lock().map_err(|e| e.to_string());
        guard.map(|s| work(&s))
    })
    .await;
    match joined {
        Ok(Ok(result)) => result,
        Ok(Err(poisoned)) => Err(internal(poisoned)),
        Err(join) => Err(internal(join.to_string())),
    }
}

#[tauri::command]
pub async fn list_workspaces(store: State<'_, AppStore>) -> Result<Vec<Workspace>, InternalError> {
    with_store(&store, InternalError::new, |s| s.list_workspaces().map_err(InternalError::new)).await
}

#[tauri::command]
pub async fn add_workspace(
    store: State<'_, AppStore>,
    name: String,
    path: String,
) -> Result<Workspace, AddWorkspaceError> {
    with_store(&store, AddWorkspaceError::Internal, move |s| {
        workspaces::add_workspace(s, &name, &path)
    })
    .await
}

/// The app's settings file. The mutex serialises read-modify-write saves.
pub type AppSettings = Arc<Mutex<SettingsFile>>;

/// Run `work` with the locked settings file on a blocking worker thread,
/// then apply the resulting snapshot to the app (theme).
async fn with_settings(
    app: &AppHandle,
    settings: &AppSettings,
    work: impl FnOnce(&SettingsFile) -> Result<Snapshot, SettingsError> + Send + 'static,
) -> Result<Snapshot, SettingsError> {
    let settings = Arc::clone(settings);
    let internal = |e: String| SettingsError::Internal { message: e };
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let file = settings.lock().map_err(|e| internal(e.to_string()))?;
        work(&file)
    })
    .await
    .map_err(|e| internal(e.to_string()))??;
    apply_settings(app, &snapshot);
    Ok(snapshot)
}

/// Make the running app reflect `snapshot`. The webview follows the window
/// theme through `prefers-color-scheme` (spec §8.4).
pub fn apply_settings(app: &AppHandle, snapshot: &Snapshot) {
    app.set_theme(match snapshot.theme() {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    });
}

/// Re-read `settings.json` (startup and window focus) and apply it.
#[tauri::command]
pub async fn get_settings(
    app: AppHandle,
    settings: State<'_, AppSettings>,
) -> Result<Snapshot, SettingsError> {
    with_settings(&app, &settings, |f| Ok(f.load())).await
}

#[tauri::command]
pub async fn set_setting(
    app: AppHandle,
    settings: State<'_, AppSettings>,
    key: String,
    value: serde_json::Value,
) -> Result<Snapshot, SettingsError> {
    with_settings(&app, &settings, move |f| f.set(&key, value)).await
}

/// Reset to defaults: replaces the file, even an invalid one.
#[tauri::command]
pub async fn reset_settings(
    app: AppHandle,
    settings: State<'_, AppSettings>,
) -> Result<Snapshot, SettingsError> {
    with_settings(&app, &settings, |f| f.reset()).await
}
