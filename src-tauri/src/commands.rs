//! The Tauri command surface the UI calls. Commands stay thin: they hand the
//! shared store to the domain modules on a blocking worker thread, so slow
//! filesystem or SQLite work never runs on (and freezes) the main thread.

use std::sync::{Arc, Mutex, MutexGuard};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::attribution::{self, Suggestion, WorkspaceStats};
use crate::scan::{self, Scanner};
use crate::settings::{SettingsError, SettingsFile, Snapshot, Theme};
use crate::agents::claude_code::{ClaudeCode, ExecutableStatus, TranscriptRootStatus};
use crate::paths::{NormalizedPath, normalize};
use crate::agents::{self, AgentStatus, LaunchMode};
use crate::launch::{self, CustomTemplate, LaunchError, OsSpawner, Terminal};
use crate::launch_env;
use crate::repository::{self, OsGit, Repository, RepositoryError};
use crate::store::{Session, Store, Workspace, WorkspaceEdit};
use crate::workspaces::{self, AddWorkspaceError, Saved, WorkspaceError};

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

/// Lock shared state, carrying on past a panic in an earlier holder: the
/// store's transactions roll back on unwind and the settings handle is just a
/// path, so neither is left half-changed.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Run `work` on a blocking worker thread, so slow filesystem or SQLite work
/// never runs on the main thread. A worker that panics becomes `internal`.
async fn blocking<R: Send + 'static, E>(
    internal: impl FnOnce(String) -> E,
    work: impl FnOnce() -> R + Send + 'static,
) -> Result<R, E> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| internal(e.to_string()))
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
    blocking(internal, move || work(&lock(&store))).await?
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
) -> Result<Saved, AddWorkspaceError> {
    with_store(&store, AddWorkspaceError::Internal, move |s| {
        workspaces::add_workspace(s, &name, &path)
    })
    .await
}

/// Locate… or a path edit: point a Workspace at a new, existing folder.
#[tauri::command]
pub async fn repoint_workspace(
    store: State<'_, AppStore>,
    id: i64,
    path: String,
) -> Result<Saved, AddWorkspaceError> {
    with_store(&store, AddWorkspaceError::Internal, move |s| workspaces::repoint_workspace(s, id, &path)).await
}

#[tauri::command]
pub async fn remove_former_path(
    store: State<'_, AppStore>,
    id: i64,
    path: String,
) -> Result<Workspace, WorkspaceError> {
    with_store(&store, WorkspaceError::Internal, move |s| workspaces::remove_former_path(s, id, &path)).await
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
    let internal = |message| SettingsError::Internal { message };
    blocking(internal, move || {
        let file = lock(&settings);
        let snapshot = work(&file)?;
        apply_settings(&app, &snapshot);
        Ok(snapshot)
    })
    .await?
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

/// The v1 Agent with the overrides in Settings (spec §5.3). Built per call,
/// from settings.json read fresh, so its executable is re-resolved every time.
fn agent(settings: &AppSettings) -> ClaudeCode {
    lock(settings).load().claude_code()
}

/// The Agent's identity, capabilities and resolved executable (or `null`).
#[tauri::command]
pub async fn agent_status(settings: State<'_, AppSettings>) -> Result<AgentStatus, InternalError> {
    let settings = Arc::clone(&settings);
    blocking(InternalError::new, move || agents::agent_status(&agent(&settings))).await
}

/// The last `--version` probe, keyed by the executable override it ran with
/// (spec §9.3: probed at startup, when Settings opens and when it changes).
pub type ExecutableProbe = Arc<Mutex<Option<(Option<String>, ExecutableStatus)>>>;

/// Settings › Agents › Claude Code status lines.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeStatus {
    pub executable: ExecutableStatus,
    pub transcript_root: TranscriptRootStatus,
}

/// The cached executable status, re-probed when `refresh` is set or the
/// executable override changed since the last probe.
pub fn probe_executable(settings: &AppSettings, cache: &ExecutableProbe, refresh: bool) -> ExecutableStatus {
    let agent = agent(settings);
    let key = agent.executable_override();
    if !refresh {
        if let Some((cached_key, status)) = lock(cache).as_ref() {
            if *cached_key == key {
                return status.clone();
            }
        }
    }
    let status = agent.executable_status();
    *lock(cache) = Some((key, status.clone()));
    status
}

#[tauri::command]
pub async fn claude_code_status(
    settings: State<'_, AppSettings>,
    probe: State<'_, ExecutableProbe>,
    refresh: bool,
) -> Result<ClaudeCodeStatus, InternalError> {
    let settings = Arc::clone(&settings);
    let probe = Arc::clone(&probe);
    blocking(InternalError::new, move || ClaudeCodeStatus {
        executable: probe_executable(&settings, &probe, refresh),
        transcript_root: agent(&settings).transcript_root_status(),
    })
    .await
}

/// The terminal chosen in Settings, read fresh so a hand edit applies at once.
fn configured(settings: &AppSettings) -> Result<(Terminal, ClaudeCode), LaunchError> {
    let snapshot = lock(settings).load();
    Ok((snapshot.terminal()?, snapshot.claude_code()))
}

#[tauri::command]
pub async fn launch_session(
    store: State<'_, AppStore>,
    settings: State<'_, AppSettings>,
    workspace_id: i64,
    mode: LaunchMode,
) -> Result<(), LaunchError> {
    let settings = Arc::clone(&settings);
    with_store(&store, LaunchError::Internal, move |s| {
        let (terminal, agent) = configured(&settings)?;
        launch::launch_session(s, &agent, terminal, &OsSpawner, &launch_env::current(), workspace_id, &mode)
    })
    .await
}

/// The event the UI listens to for scan progress; the payload is `true` when
/// a scan starts and `false` when it (and any coalesced follow-up) finishes.
pub const SCAN_STATE_EVENT: &str = "scan-state";

/// The app's background scanner over the Claude Code transcripts. Each scan
/// reads the transcript root from settings afresh (spec §9.4).
pub fn scanner(app: AppHandle, store: AppStore, settings: AppSettings) -> Scanner {
    Scanner::new(
        move |full| {
            let agent = agent(&settings);
            let result = if full { scan::run_full(&store, &agent) } else { scan::run(&store, &agent) };
            if let Err(e) = result {
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

/// Settings › Suggestions: the dismissed paths, oldest first.
#[tauri::command]
pub async fn list_dismissed_paths(store: State<'_, AppStore>) -> Result<Vec<NormalizedPath>, InternalError> {
    with_store(&store, InternalError::new, |s| s.list_dismissed_paths().map_err(InternalError::new)).await
}

/// Settings › Suggestions › Restore: the path can come back as a Suggestion.
#[tauri::command]
pub async fn restore_dismissed_path(store: State<'_, AppStore>, path: String) -> Result<(), WorkspaceError> {
    with_store(&store, WorkspaceError::Internal, move |s| {
        let path = normalize(&path).map_err(|e| WorkspaceError::InvalidPath(e.to_string()))?;
        Ok(s.restore_dismissed_path(&path)?)
    })
    .await
}

/// Settings › Data › Rescan transcripts: re-parse every transcript (spec §5.1).
/// Returns at once; progress arrives as [`SCAN_STATE_EVENT`].
#[tauri::command]
pub fn rescan_transcripts(scanner: State<'_, Scanner>) {
    scanner.trigger_full();
}

/// Settings › Data › Open data folder: show the app data folder (spec §4.1)
/// in the system file manager.
#[tauri::command]
pub async fn open_data_folder(app: AppHandle) -> Result<(), InternalError> {
    let dir = app.path().app_data_dir().map_err(InternalError::new)?;
    launch::open_folder(&OsSpawner, &dir.to_string_lossy()).map_err(InternalError::new)
}

/// Settings › Terminal › Test: run `claude --version` in the home directory
/// through the custom argv `template` (spec §9.2).
#[tauri::command]
pub async fn test_terminal(
    app: AppHandle,
    settings: State<'_, AppSettings>,
    template: Vec<String>,
) -> Result<(), LaunchError> {
    let template = CustomTemplate::new(template).map_err(|message| LaunchError::TerminalNotConfigured { message })?;
    let home = app.path().home_dir().map_err(|e| LaunchError::Internal(e.to_string()))?;
    let home = home.to_string_lossy().into_owned();
    let settings = Arc::clone(&settings);
    blocking(LaunchError::Internal, move || {
        launch::test_terminal(&agent(&settings), &Terminal::Custom(template), &OsSpawner, &launch_env::current(), &home)
    })
    .await?
}

/// The Workspace's git repository for the detail header (#32), or `null`.
#[tauri::command]
pub async fn workspace_repository(
    store: State<'_, AppStore>,
    workspace_id: i64,
) -> Result<Option<Repository>, RepositoryError> {
    let workspace = read_workspace(&store, workspace_id).await?;
    blocking(RepositoryError::Internal, move || repository::workspace_repository(&workspace, &OsGit)).await
}

/// Open the Workspace's repository page in the default browser. Takes the id,
/// not a URL: the page is re-resolved here (ADR 0001).
#[tauri::command]
pub async fn open_workspace_repository(store: State<'_, AppStore>, workspace_id: i64) -> Result<(), RepositoryError> {
    let workspace = read_workspace(&store, workspace_id).await?;
    blocking(RepositoryError::Internal, move || {
        repository::open_workspace_repository(&workspace, &OsGit, &OsSpawner)
    })
    .await?
}

/// Read the Workspace under the store lock; git then runs without it, so a
/// slow git never holds up other commands.
async fn read_workspace(store: &AppStore, workspace_id: i64) -> Result<Workspace, RepositoryError> {
    with_store(store, RepositoryError::Internal, move |s| repository::get_workspace(s, workspace_id)).await
}
