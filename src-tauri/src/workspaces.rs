//! Workspace operations that combine path rules, the filesystem and the store
//! (spec §6.2). Commands call these; they hold no Tauri types.

use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

use crate::paths::{NormalizedPath, PathError, normalize};
use crate::store::{NewWorkspace, Store, StoreError, Workspace, WorkspaceEdit};

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
    #[error("That Workspace no longer exists")]
    NoSuchWorkspace,
    #[error("{0}")]
    Internal(String),
}

/// A Workspace saved at a path, and the Workspace that path was moved off
/// (it was one of its former paths), for the inline notice (spec §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub workspace: Workspace,
    pub moved_from: Option<MovedFrom>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovedFrom {
    pub workspace_id: i64,
    pub workspace_name: String,
}

/// Normalise `raw_path`, check the directory exists now, and save the Workspace.
pub fn add_workspace(store: &Store, name: &str, raw_path: &str) -> Result<Saved, AddWorkspaceError> {
    if name.trim().is_empty() {
        return Err(AddWorkspaceError::EmptyName);
    }
    let path = existing_directory(raw_path)?;
    let moved_from = former_owner(store, &path, None)?;
    let workspace = store.add_workspace(NewWorkspace { name: name.to_owned(), path })?;
    Ok(Saved { workspace, moved_from })
}

/// Re-point a Workspace (Locate… or a path edit, spec §6.5). The directory
/// must exist now; the app never creates it. The old path becomes a former path.
pub fn repoint_workspace(store: &Store, id: i64, raw_path: &str) -> Result<Saved, AddWorkspaceError> {
    let path = existing_directory(raw_path)?;
    let moved_from = former_owner(store, &path, Some(id))?;
    let workspace = store.repoint_workspace(id, &path)?;
    Ok(Saved { workspace, moved_from })
}

/// Remove one of a Workspace's former paths (edit dialog, spec §6.2).
pub fn remove_former_path(store: &Store, id: i64, raw_path: &str) -> Result<Workspace, WorkspaceError> {
    let path = normalize(raw_path).map_err(|e| WorkspaceError::InvalidPath(e.to_string()))?;
    Ok(store.remove_former_path(id, &path)?)
}

fn existing_directory(raw_path: &str) -> Result<NormalizedPath, AddWorkspaceError> {
    let path = normalize(raw_path)?;
    match std::fs::metadata(path.as_path()) {
        Ok(meta) if meta.is_dir() => Ok(path),
        Ok(_) => Err(AddWorkspaceError::NotADirectory { path: path.to_string() }),
        Err(_) => Err(AddWorkspaceError::NotFound { path: path.to_string() }),
    }
}

/// Another Workspace (not `except`) that has `path` as a former path.
fn former_owner(store: &Store, path: &NormalizedPath, except: Option<i64>) -> Result<Option<MovedFrom>, StoreError> {
    Ok(store
        .former_path_owner(path)?
        .filter(|(id, _)| Some(*id) != except)
        .map(|(workspace_id, workspace_name)| MovedFrom { workspace_id, workspace_name }))
}

/// Why a Workspace could not be edited, pinned or removed. Serialises for the
/// UI as `{ kind, message }`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WorkspaceError {
    #[error("A Workspace needs a name")]
    EmptyName,
    #[error("That Workspace no longer exists")]
    NoSuchWorkspace,
    #[error("{0}")]
    InvalidPath(String),
    #[error("{0}")]
    Internal(String),
}

/// Save the edit dialog's name, note and tags (spec §6.2).
pub fn update_workspace(store: &Store, id: i64, edit: WorkspaceEdit) -> Result<Workspace, WorkspaceError> {
    Ok(store.update_workspace(id, edit)?)
}

pub fn set_pinned(store: &Store, id: i64, pinned: bool) -> Result<Workspace, WorkspaceError> {
    Ok(store.set_pinned(id, pinned)?)
}

/// Dismiss a Suggestion's path so it never comes back (spec §6.6).
pub fn dismiss_path(store: &Store, raw_path: &str) -> Result<(), WorkspaceError> {
    let path = normalize(raw_path).map_err(|e| WorkspaceError::InvalidPath(e.to_string()))?;
    Ok(store.dismiss_path(&path)?)
}

/// Remove a Workspace with its tags, dismissing its path (spec §4.3).
pub fn remove_workspace(store: &Store, id: i64) -> Result<(), WorkspaceError> {
    Ok(store.remove_workspace(id)?)
}

impl From<StoreError> for WorkspaceError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::EmptyName => WorkspaceError::EmptyName,
            StoreError::NoSuchWorkspace(_) => WorkspaceError::NoSuchWorkspace,
            other => WorkspaceError::Internal(other.to_string()),
        }
    }
}

impl Serialize for WorkspaceError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        let kind = match self {
            Self::EmptyName => "emptyName",
            Self::NoSuchWorkspace => "noSuchWorkspace",
            Self::InvalidPath(_) => "invalidPath",
            Self::Internal(_) => "internal",
        };
        map.serialize_entry("kind", kind)?;
        map.serialize_entry("message", &self.to_string())?;
        map.end()
    }
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
            StoreError::NoSuchWorkspace(_) => AddWorkspaceError::NoSuchWorkspace,
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
            Self::NoSuchWorkspace => "noSuchWorkspace",
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
