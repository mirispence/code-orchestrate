//! Read-time Workspace matching and Suggestions (spec §5.4, §5.5).
//! Attribution is never stored: it is computed from stored Sessions and the
//! current Workspaces every time it is read.

use std::collections::HashMap;

use serde::Serialize;

use crate::agents::TokenUsage;
use crate::paths::NormalizedPath;
use crate::store::{Session, Store, StoreError, Workspace};

/// The Workspace a Session in `cwd` belongs to: the one whose path equals or
/// is an ancestor of `cwd`, the deepest if several match.
pub fn owner<'a>(cwd: &NormalizedPath, workspaces: &'a [Workspace]) -> Option<&'a Workspace> {
    workspaces
        .iter()
        .filter(|w| w.path.same_as(cwd) || w.path.is_ancestor_of(cwd))
        .max_by_key(|w| w.path.as_str().len())
}

/// A directory where Sessions ran that no Workspace covers and that has not
/// been dismissed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub path: NormalizedPath,
    pub session_count: u32,
    /// The newest `last_activity_at`, Unix epoch milliseconds.
    pub last_seen: Option<i64>,
    /// The directory no longer exists; it can only be dismissed.
    pub missing: bool,
}

/// Suggestions, most recently seen first. Checks each directory's existence.
pub fn suggestions(store: &Store) -> Result<Vec<Suggestion>, StoreError> {
    let workspaces = store.list_workspaces()?;
    let dismissed: Vec<String> = store.list_dismissed_paths()?.iter().map(|p| p.comparison_key()).collect();

    let mut by_key: HashMap<String, Suggestion> = HashMap::new();
    for session in store.list_sessions()? {
        let cwd = session.first_cwd;
        if owner(&cwd, &workspaces).is_some() {
            continue;
        }
        let key = cwd.comparison_key();
        if dismissed.contains(&key) {
            continue;
        }
        let entry = by_key.entry(key).or_insert_with(|| Suggestion {
            path: cwd.clone(),
            session_count: 0,
            last_seen: None,
            missing: false,
        });
        entry.session_count += 1;
        entry.last_seen = entry.last_seen.max(session.last_activity_at);
    }

    let mut list: Vec<Suggestion> = by_key.into_values().collect();
    for s in &mut list {
        s.missing = !s.path.as_path().is_dir();
    }
    list.sort_by(|a, b| b.last_seen.cmp(&a.last_seen).then_with(|| a.path.as_str().cmp(b.path.as_str())));
    Ok(list)
}

/// One Workspace's all-time totals over its attributed Sessions (spec §5.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStats {
    pub workspace_id: i64,
    pub session_count: u32,
    /// The newest `last_activity_at`, Unix epoch milliseconds.
    pub last_used: Option<i64>,
    /// Sum of active time; `None` when no Session reports it.
    pub session_time_ms: Option<i64>,
    /// Field-wise sums; a field is `None` when no Session reports it.
    pub tokens: TokenUsage,
}

/// Stats for every Workspace, in `list_workspaces` order. Each Session counts
/// toward its owner only (the deepest matching Workspace).
pub fn workspace_stats(store: &Store) -> Result<Vec<WorkspaceStats>, StoreError> {
    let workspaces = store.list_workspaces()?;
    let mut stats: Vec<WorkspaceStats> = workspaces
        .iter()
        .map(|w| WorkspaceStats {
            workspace_id: w.id,
            session_count: 0,
            last_used: None,
            session_time_ms: None,
            tokens: TokenUsage::default(),
        })
        .collect();
    let index: HashMap<i64, usize> = workspaces.iter().enumerate().map(|(i, w)| (w.id, i)).collect();
    for session in store.list_sessions()? {
        let Some(owner) = owner(&session.first_cwd, &workspaces) else { continue };
        let s = &mut stats[index[&owner.id]];
        s.session_count += 1;
        s.last_used = s.last_used.max(session.last_activity_at);
        if let Some(ms) = session.active_time_ms {
            s.session_time_ms = Some(s.session_time_ms.unwrap_or(0) + ms);
        }
        s.tokens = s.tokens.plus(session.tokens);
    }
    Ok(stats)
}

/// The Sessions attributed to a Workspace, newest last activity first.
pub fn workspace_sessions(store: &Store, workspace_id: i64) -> Result<Vec<Session>, StoreError> {
    let workspaces = store.list_workspaces()?;
    if !workspaces.iter().any(|w| w.id == workspace_id) {
        return Err(StoreError::NoSuchWorkspace(workspace_id));
    }
    let mut sessions: Vec<Session> = store
        .list_sessions()?
        .into_iter()
        .filter(|s| owner(&s.first_cwd, &workspaces).is_some_and(|w| w.id == workspace_id))
        .collect();
    sessions.sort_by(|a, b| {
        b.last_activity_at.cmp(&a.last_activity_at).then_with(|| a.session_id.cmp(&b.session_id))
    });
    Ok(sessions)
}
