//! The Tauri command surface the UI calls. Commands stay thin: they hand the
//! shared store to the domain modules on a blocking worker thread, so slow
//! filesystem or SQLite work never runs on (and freezes) the main thread.

use std::sync::{Arc, Mutex};

use tauri::State;

use crate::agents::claude_code::ClaudeCode;
use crate::agents::{self, AgentStatus, LaunchMode};
use crate::launch::{self, LaunchError, OsSpawner, Terminal};
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

#[tauri::command]
pub async fn launch_session(
    store: State<'_, AppStore>,
    workspace_id: i64,
    mode: SessionStart,
) -> Result<(), LaunchError> {
    let mode = match mode {
        SessionStart::New => LaunchMode::New,
        SessionStart::Continue => LaunchMode::Continue,
    };
    with_store(&store, LaunchError::Internal, move |s| {
        launch::launch_session(s, &agent(), Terminal::WindowsTerminalNewWindow, &OsSpawner, workspace_id, &mode)
    })
    .await
}
