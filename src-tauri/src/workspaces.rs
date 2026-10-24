//! Workspace operations that combine path rules, the filesystem and the store
//! (spec §6.2). Commands call these; they hold no Tauri types.

use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

use crate::paths::{PathError, normalize};
use crate::store::{NewWorkspace, Store, StoreError, Workspace};

/// Why a Workspace could not be added. Serialises for the UI as
/// `{ kind, message, ...fields }` with camelCase keys.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AddWorkspaceError {
    #[error("A Workspace needs a name")]
    EmptyName,
    #[error("{0}")]
    InvalidPath(String),
    #[error("Folder not found: {path}")]
    NotFound { path: String },
    #[error("Not a folder: {path}")]
    NotADirectory { path: String },
    #[error("Already a Workspace: {workspace_name}")]
    PathTaken { workspace_id: i64, workspace_name: String },
    #[error("{0}")]
    Internal(String),
}

/// Normalise `raw_path`, check the directory exists now, and save the Workspace.
pub fn add_workspace(store: &Store, name: &str, raw_path: &str) -> Result<Workspace, AddWorkspaceError> {
    if name.trim().is_empty() {
        return Err(AddWorkspaceError::EmptyName);
    }
    let path = normalize(raw_path)?;
    match std::fs::metadata(path.as_path()) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(AddWorkspaceError::NotADirectory { path: path.to_string() }),
        Err(_) => return Err(AddWorkspaceError::NotFound { path: path.to_string() }),
    }
    Ok(store.add_workspace(NewWorkspace { name: name.to_owned(), path })?)
}

impl From<PathError> for AddWorkspaceError {
    fn from(e: PathError) -> Self {
        AddWorkspaceError::InvalidPath(e.to_string())
    }
}

impl From<StoreError> for AddWorkspaceError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::EmptyName => AddWorkspaceError::EmptyName,
            StoreError::PathTaken { workspace_id, workspace_name } => {
                AddWorkspaceError::PathTaken { workspace_id, workspace_name }
            }
            other => AddWorkspaceError::Internal(other.to_string()),
        }
    }
}

impl Serialize for AddWorkspaceError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        let kind = match self {
            Self::EmptyName => "emptyName",
            Self::InvalidPath(_) => "invalidPath",
            Self::NotFound { .. } => "notFound",
            Self::NotADirectory { .. } => "notADirectory",
            Self::PathTaken { .. } => "pathTaken",
            Self::Internal(_) => "internal",
        };
        map.serialize_entry("kind", kind)?;
        map.serialize_entry("message", &self.to_string())?;
        match self {
            Self::NotFound { path } | Self::NotADirectory { path } => {
                map.serialize_entry("path", path)?;
            }
            Self::PathTaken { workspace_id, workspace_name } => {
                map.serialize_entry("workspaceId", workspace_id)?;
                map.serialize_entry("workspaceName", workspace_name)?;
            }
            _ => {}
        }
        map.end()
    }
}
