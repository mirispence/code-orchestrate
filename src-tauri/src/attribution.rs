//! Read-time Workspace matching and Suggestions (spec §5.4, §5.5).
//! Attribution is never stored: it is computed from stored Sessions and the
//! current Workspaces every time it is read.

use std::collections::HashMap;

use serde::Serialize;

use crate::paths::NormalizedPath;
use crate::store::{Store, StoreError, Workspace};

/// The Workspace a Session in `cwd` belongs to: the one with a current or
/// former path that equals or is an ancestor of `cwd`, the deepest path
/// winning if several match (spec §5.4, §6.5).
pub fn owner<'a>(cwd: &NormalizedPath, workspaces: &'a [Workspace]) -> Option<&'a Workspace> {
    workspaces
        .iter()
        .flat_map(|w| std::iter::once(&w.path).chain(&w.former_paths).map(move |p| (w, p)))
        .filter(|(_, p)| p.same_as(cwd) || p.is_ancestor_of(cwd))
        .max_by_key(|(_, p)| p.as_str().len())
        .map(|(w, _)| w)
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
