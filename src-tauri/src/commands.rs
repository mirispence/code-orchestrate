//! The Tauri command surface the UI calls. Commands stay thin: they lock the
//! shared store and delegate to the domain modules.

use std::sync::Mutex;

use tauri::State;

use crate::store::{Store, Workspace};
use crate::workspaces::{self, AddWorkspaceError};

/// The app's single store, shared across commands.
pub type AppStore = Mutex<Store>;

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

#[tauri::command]
pub fn list_workspaces(store: State<'_, AppStore>) -> Result<Vec<Workspace>, InternalError> {
    let store = store.lock().map_err(InternalError::new)?;
    store.list_workspaces().map_err(InternalError::new)
}

#[tauri::command]
pub fn add_workspace(
    store: State<'_, AppStore>,
    name: String,
    path: String,
) -> Result<Workspace, AddWorkspaceError> {
    let store = store.lock().map_err(|e| AddWorkspaceError::Internal(e.to_string()))?;
    workspaces::add_workspace(&store, &name, &path)
}
