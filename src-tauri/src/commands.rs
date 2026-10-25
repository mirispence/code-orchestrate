//! The Tauri command surface the UI calls. Commands stay thin: they hand the
//! shared store to the domain modules on a blocking worker thread, so slow
//! filesystem or SQLite work never runs on (and freezes) the main thread.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::attribution::{self, Suggestion, WorkspaceStats};
use crate::scan::{self, Scanner};
use crate::settings::{SettingsError, SettingsFile, Snapshot, Theme};
use crate::agents::claude_code::ClaudeCode;
use crate::agents::{self, AgentStatus, LaunchMode};
use crate::launch::{self, CustomTemplate, LaunchError, OsSpawner, Terminal};
use crate::store::{Session, Store, Workspace, WorkspaceEdit};
use crate::workspaces::{self, AddWorkspaceError, WorkspaceError};

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
pub async fn update_workspace(
    store: State<'_, AppStore>,
    id: i64,
    name: String,
    note: Option<String>,
    tags: Vec<String>,
) -> Result<Workspace, WorkspaceError> {
    with_store(&store, WorkspaceError::Internal, move |s| {
        workspaces::update_workspace(s, id, WorkspaceEdit { name, note, tags })
    })
    .await
}

#[tauri::command]
pub async fn set_workspace_pinned(
    store: State<'_, AppStore>,
    id: i64,
    pinned: bool,
) -> Result<Workspace, WorkspaceError> {
    with_store(&store, WorkspaceError::Internal, move |s| workspaces::set_pinned(s, id, pinned)).await
}

#[tauri::command]
pub async fn remove_workspace(store: State<'_, AppStore>, id: i64) -> Result<(), WorkspaceError> {
    with_store(&store, WorkspaceError::Internal, move |s| workspaces::remove_workspace(s, id)).await
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

/// Run `work` with the locked settings file on a blocking worker thread and
/// apply the resulting snapshot (theme) before releasing the lock, so
/// overlapping operations apply in the same order they touched the file.
async fn with_settings(
    app: &AppHandle,
    settings: &AppSettings,
    work: impl FnOnce(&SettingsFile) -> Result<Snapshot, SettingsError> + Send + 'static,
) -> Result<Snapshot, SettingsError> {
    let settings = Arc::clone(settings);
    let app = app.clone();
    let internal = |e: String| SettingsError::Internal { message: e };
    tauri::async_runtime::spawn_blocking(move || {
        let file = settings.lock().map_err(|e| internal(e.to_string()))?;
        let snapshot = work(&file)?;
        apply_settings(&app, &snapshot);
        Ok(snapshot)
    })
    .await
    .map_err(|e| internal(e.to_string()))?
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

/// The v1 Agent. Built per call so its executable is re-resolved every time.
fn agent() -> ClaudeCode {
    ClaudeCode::on_path()
}

/// The Agent's identity, capabilities and resolved executable (or `null`).
#[tauri::command]
pub async fn agent_status() -> Result<AgentStatus, InternalError> {
    tauri::async_runtime::spawn_blocking(|| agents::agent_status(&agent()))
        .await
        .map_err(InternalError::new)
}

/// How the UI asks to start a Session: `"new"` or `"continue"`.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionStart {
    New,
    Continue,
}

/// The terminal chosen in Settings, read fresh so a hand edit applies at once.
fn configured_terminal(settings: &AppSettings) -> Result<Terminal, LaunchError> {
    let file = settings.lock().map_err(|e| LaunchError::Internal(e.to_string()))?;
    file.load().terminal()
}

#[tauri::command]
pub async fn launch_session(
    store: State<'_, AppStore>,
    settings: State<'_, AppSettings>,
    workspace_id: i64,
    mode: SessionStart,
) -> Result<(), LaunchError> {
    let mode = match mode {
        SessionStart::New => LaunchMode::New,
        SessionStart::Continue => LaunchMode::Continue,
    };
    let settings = Arc::clone(&settings);
    with_store(&store, LaunchError::Internal, move |s| {
        let terminal = configured_terminal(&settings)?;
        launch::launch_session(s, &agent(), terminal, &OsSpawner, workspace_id, &mode)
    })
    .await
}

/// The event the UI listens to for scan progress; the payload is `true` when
/// a scan starts and `false` when it (and any coalesced follow-up) finishes.
pub const SCAN_STATE_EVENT: &str = "scan-state";

/// The app's background scanner over the Claude Code transcripts.
pub fn scanner(app: AppHandle, store: AppStore) -> Scanner {
    Scanner::new(
        move || {
            if let Err(e) = scan::run(&store, &agent()) {
                eprintln!("transcript scan failed: {e}");
            }
        },
        move |running| {
            let _ = app.emit(SCAN_STATE_EVENT, running);
        },
    )
}

/// Start a scan (the sidebar refresh button). Returns at once; progress
/// arrives as [`SCAN_STATE_EVENT`].
#[tauri::command]
pub fn start_scan(scanner: State<'_, Scanner>) {
    scanner.trigger();
}

/// Whether a scan is running now (for a UI that mounts mid-scan).
#[tauri::command]
pub fn scan_running(scanner: State<'_, Scanner>) -> bool {
    scanner.is_running()
}

#[tauri::command]
pub async fn list_suggestions(store: State<'_, AppStore>) -> Result<Vec<Suggestion>, InternalError> {
    with_store(&store, InternalError::new, |s| attribution::suggestions(s).map_err(InternalError::new)).await
}

/// Every Workspace's stats, computed from stored Sessions (spec §5.6).
#[tauri::command]
pub async fn workspace_stats(store: State<'_, AppStore>) -> Result<Vec<WorkspaceStats>, InternalError> {
    with_store(&store, InternalError::new, |s| attribution::workspace_stats(s).map_err(InternalError::new)).await
}

/// A Workspace's Sessions, newest last activity first (spec §8.2).
#[tauri::command]
pub async fn list_workspace_sessions(
    store: State<'_, AppStore>,
    workspace_id: i64,
) -> Result<Vec<Session>, InternalError> {
    with_store(&store, InternalError::new, move |s| {
        attribution::workspace_sessions(s, workspace_id).map_err(InternalError::new)
    })
    .await
}

#[tauri::command]
pub async fn dismiss_path(store: State<'_, AppStore>, path: String) -> Result<(), WorkspaceError> {
    with_store(&store, WorkspaceError::Internal, move |s| workspaces::dismiss_path(s, &path)).await
}

/// Settings › Terminal › Test: run `claude --version` in the home directory
/// through the custom argv `template` (spec §9.2).
#[tauri::command]
pub async fn test_terminal(app: AppHandle, template: Vec<String>) -> Result<(), LaunchError> {
    let template = CustomTemplate::new(template).map_err(|message| LaunchError::TerminalNotConfigured { message })?;
    let home = app.path().home_dir().map_err(|e| LaunchError::Internal(e.to_string()))?;
    let home = home.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        launch::test_terminal(&agent(), &Terminal::Custom(template), &OsSpawner, &home)
    })
    .await
    .map_err(|e| LaunchError::Internal(e.to_string()))?
}
