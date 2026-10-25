//! SQLite access and migrations (spec §4). The rest of the app sees typed
//! records and domain errors; SQL stays in this module.
//!
//! Adding a table or column: append a migration to [`MIGRATIONS`] (never edit
//! a shipped one). Path uniqueness across Workspace current (and, later,
//! former) paths is enforced here by each row's `path_key`, the path's
//! [`NormalizedPath::comparison_key`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use crate::agents::{SessionSummary, SourceKey, TokenUsage};
use crate::paths::NormalizedPath;

/// Ordered schema migrations. Migration `i` takes the DB from version `i` to `i + 1`.
const MIGRATIONS: &[&str] = &[
    // 1: Workspaces.
    "CREATE TABLE workspace (
        id          INTEGER PRIMARY KEY,
        name        TEXT    NOT NULL CHECK (trim(name) <> ''),
        path        TEXT    NOT NULL,
        path_key    TEXT    NOT NULL UNIQUE,
        pinned      INTEGER NOT NULL DEFAULT 0,
        note        TEXT,
        created_at  INTEGER NOT NULL
    );",
    // 2: WorkspaceTag. `tag_key` is the case-folded tag; `position` keeps the typed order.
    "CREATE TABLE workspace_tag (
        workspace_id INTEGER NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
        tag          TEXT    NOT NULL CHECK (trim(tag) <> ''),
        tag_key      TEXT    NOT NULL,
        position     INTEGER NOT NULL,
        PRIMARY KEY (workspace_id, tag_key)
    );
    CREATE INDEX workspace_tag_by_key ON workspace_tag (tag_key);",
    // 3: Session, SourceFile and DismissedPath (spec §4.2). Session records are
    // durable: a scan inserts or updates them and never deletes one. The stats
    // columns stay null until a scan fills them.
    "CREATE TABLE session (
        agent_id            TEXT    NOT NULL,
        session_id          TEXT    NOT NULL,
        first_cwd           TEXT    NOT NULL,
        title               TEXT,
        started_at          INTEGER,
        last_activity_at    INTEGER,
        active_time_ms      INTEGER,
        tokens_input        INTEGER,
        tokens_output       INTEGER,
        tokens_cache_write  INTEGER,
        tokens_cache_read   INTEGER,
        source_path         TEXT    NOT NULL,
        PRIMARY KEY (agent_id, session_id)
    );
    CREATE TABLE source_file (
        path      TEXT    PRIMARY KEY,
        agent_id  TEXT    NOT NULL,
        mtime     INTEGER NOT NULL,
        size      INTEGER NOT NULL
    );
    CREATE TABLE dismissed_path (
        path          TEXT    NOT NULL,
        path_key      TEXT    PRIMARY KEY,
        dismissed_at  INTEGER NOT NULL
    );",
];

/// The schema version this build migrates to.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("A Workspace needs a name")]
    EmptyName,
    #[error("Already a Workspace: {workspace_name}")]
    PathTaken { workspace_id: i64, workspace_name: String },
    #[error("No such Workspace: {0}")]
    NoSuchWorkspace(i64),
    #[error("The database is at schema version {found}, newer than this app's {supported}")]
    NewerSchema { found: i64, supported: i64 },
    #[error("Could not back up the database before migrating: {0}")]
    Backup(#[source] std::io::Error),
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: i64,
    pub name: String,
    pub path: NormalizedPath,
    pub pinned: bool,
    pub note: Option<String>,
    /// Unix epoch milliseconds.
    pub created_at: i64,
    /// In the order they were added; unique case-insensitively.
    pub tags: Vec<String>,
}

/// The user-editable fields of a Workspace, as saved from the edit dialog.
#[derive(Debug, Clone)]
pub struct WorkspaceEdit {
    pub name: String,
    pub note: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NewWorkspace {
    pub name: String,
    pub path: NormalizedPath,
}

/// A stored Session (spec §4.2). Times are Unix epoch milliseconds, UTC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub agent_id: String,
    pub session_id: String,
    pub first_cwd: NormalizedPath,
    pub title: Option<String>,
    pub started_at: Option<i64>,
    pub last_activity_at: Option<i64>,
    pub active_time_ms: Option<i64>,
    /// Each field `None` when the Agent does not report it.
    pub tokens: TokenUsage,
    pub source_path: String,
}

/// What one scanned transcript yielded: its source key and, if it holds a
/// Session yet, the parsed summary.
#[derive(Debug, Clone)]
pub struct ScannedSource {
    pub key: SourceKey,
    pub session: Option<SessionSummary>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (creating if needed) the database file and bring it to
    /// [`SCHEMA_VERSION`], copying it to `<file>.bak` before any migration.
    pub fn open(db_path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let store = Store { conn };
        store.migrate(Some(db_path))?;
        Ok(store)
    }

    /// An in-memory store at the latest schema, for tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let store = Store { conn };
        store.migrate(None)?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<i64, StoreError> {
        Ok(self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    fn migrate(&self, db_path: Option<&Path>) -> Result<(), StoreError> {
        let current = self.schema_version()?;
        if current > SCHEMA_VERSION {
            return Err(StoreError::NewerSchema { found: current, supported: SCHEMA_VERSION });
        }
        if current == SCHEMA_VERSION {
            return Ok(());
        }
        if let Some(path) = db_path {
            std::fs::copy(path, backup_path(path)).map_err(StoreError::Backup)?;
        }
        for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", index as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Workspaces sorted by name, case-insensitively.
    pub fn list_workspaces(&self) -> Result<Vec<Workspace>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, path, pinned, note, created_at FROM workspace
             ORDER BY name COLLATE NOCASE, id",
        )?;
        let mut workspaces: Vec<Workspace> =
            stmt.query_map([], workspace_from_row)?.collect::<Result<_, _>>()?;
        let mut tags = self.conn.prepare(
            "SELECT workspace_id, tag FROM workspace_tag ORDER BY workspace_id, position",
        )?;
        let index: HashMap<i64, usize> = workspaces.iter().enumerate().map(|(i, w)| (w.id, i)).collect();
        for row in tags.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (id, tag) = row?;
            if let Some(&i) = index.get(&id) {
                workspaces[i].tags.push(tag);
            }
        }
        Ok(workspaces)
    }

    /// Every tag some Workspace uses, sorted case-insensitively (for autocomplete).
    pub fn list_tags(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT MIN(tag) FROM workspace_tag GROUP BY tag_key ORDER BY tag_key",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Insert a Workspace. Rejects an empty name and a path another Workspace owns.
    pub fn add_workspace(&self, new: NewWorkspace) -> Result<Workspace, StoreError> {
        let name = new.name.trim();
        if name.is_empty() {
            return Err(StoreError::EmptyName);
        }
        let key = new.path.comparison_key();
        if let Some((workspace_id, workspace_name)) = self.path_owner(&key)? {
            return Err(StoreError::PathTaken { workspace_id, workspace_name });
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO workspace (name, path, path_key, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![name, new.path.as_str(), key, now_millis()],
        )?;
        let id = tx.last_insert_rowid();
        // Adding a Workspace at a dismissed path restores it (spec §6.6).
        tx.execute("DELETE FROM dismissed_path WHERE path_key = ?1", [&key])?;
        tx.commit()?;
        self.get_workspace(id)
    }

    /// Change a Workspace's name, note and tags (spec §6.2). The name is
    /// trimmed and must be non-empty; a blank note is stored as none.
    pub fn update_workspace(&self, id: i64, edit: WorkspaceEdit) -> Result<Workspace, StoreError> {
        let name = edit.name.trim();
        if name.is_empty() {
            return Err(StoreError::EmptyName);
        }
        let note = edit.note.as_deref().filter(|n| !n.trim().is_empty());
        let tx = self.conn.unchecked_transaction()?;
        let changed =
            tx.execute("UPDATE workspace SET name = ?1, note = ?2 WHERE id = ?3", params![name, note, id])?;
        if changed == 0 {
            return Err(StoreError::NoSuchWorkspace(id));
        }
        // A tag already in use keeps the spelling it was first typed with.
        let mut tags = Vec::new();
        let mut seen = HashSet::new();
        for raw in &edit.tags {
            let key = tag_key(raw);
            if key.is_empty() || !seen.insert(key.clone()) {
                continue;
            }
            let stored: Option<String> = tx
                .query_row("SELECT tag FROM workspace_tag WHERE tag_key = ?1 LIMIT 1", [&key], |r| r.get(0))
                .optional()?;
            tags.push((stored.unwrap_or_else(|| raw.trim().to_owned()), key));
        }
        tx.execute("DELETE FROM workspace_tag WHERE workspace_id = ?1", [id])?;
        for (position, (tag, key)) in tags.iter().enumerate() {
            tx.execute(
                "INSERT INTO workspace_tag (workspace_id, tag, tag_key, position) VALUES (?1, ?2, ?3, ?4)",
                params![id, tag, key, position as i64],
            )?;
        }
        tx.commit()?;
        self.get_workspace(id)
    }

    /// Pin or unpin a Workspace (spec §8.1).
    pub fn set_pinned(&self, id: i64, pinned: bool) -> Result<Workspace, StoreError> {
        let changed = self.conn.execute("UPDATE workspace SET pinned = ?1 WHERE id = ?2", params![pinned, id])?;
        if changed == 0 {
            return Err(StoreError::NoSuchWorkspace(id));
        }
        self.get_workspace(id)
    }

    /// Delete a Workspace's row; its tags go with it (ON DELETE CASCADE). Its
    /// Session records stay, and its path is dismissed so it does not come
    /// back as a Suggestion (spec §4.3).
    pub fn remove_workspace(&self, id: i64) -> Result<(), StoreError> {
        let ws = self.get_workspace(id)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM workspace WHERE id = ?1", [id])?;
        insert_dismissed(&tx, &ws.path)?;
        tx.commit()?;
        Ok(())
    }

    /// Hide a path from Suggestions permanently (spec §6.6). Dismissing an
    /// already dismissed path keeps its original time.
    pub fn dismiss_path(&self, path: &NormalizedPath) -> Result<(), StoreError> {
        insert_dismissed(&self.conn, path)
    }

    /// Dismissed paths, oldest dismissal first.
    pub fn list_dismissed_paths(&self) -> Result<Vec<NormalizedPath>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT path FROM dismissed_path ORDER BY dismissed_at, path")?;
        let rows = stmt.query_map([], |r| Ok(NormalizedPath::from_stored(r.get(0)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every stored Session, matched to a Workspace or not.
    pub fn list_sessions(&self) -> Result<Vec<Session>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT agent_id, session_id, first_cwd, title, started_at, last_activity_at, active_time_ms,
                    tokens_input, tokens_output, tokens_cache_write, tokens_cache_read, source_path
             FROM session ORDER BY agent_id, session_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Session {
                agent_id: r.get(0)?,
                session_id: r.get(1)?,
                first_cwd: NormalizedPath::from_stored(r.get(2)?),
                title: r.get(3)?,
                started_at: r.get(4)?,
                last_activity_at: r.get(5)?,
                active_time_ms: r.get(6)?,
                tokens: TokenUsage {
                    input: r.get(7)?,
                    output: r.get(8)?,
                    cache_write: r.get(9)?,
                    cache_read: r.get(10)?,
                },
                source_path: r.get(11)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The source key of every transcript an Agent's scans have parsed, by path.
    pub fn source_keys(&self, agent_id: &str) -> Result<HashMap<String, (i64, u64)>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT path, mtime, size FROM source_file WHERE agent_id = ?1")?;
        let rows = stmt.query_map([agent_id], |r| {
            Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?, r.get::<_, i64>(2)? as u64)))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Save what a scan parsed, in one transaction: upsert each Session and
    /// record each file's source key. Nothing is ever deleted (spec §4.3).
    pub fn record_scan(&self, agent_id: &str, scanned: &[ScannedSource]) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        for source in scanned {
            let path = source.key.path.to_string_lossy();
            if let Some(s) = &source.session {
                tx.execute(
                    "INSERT INTO session (agent_id, session_id, first_cwd, title, started_at, last_activity_at,
                        active_time_ms, tokens_input, tokens_output, tokens_cache_write, tokens_cache_read, source_path)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                     ON CONFLICT (agent_id, session_id) DO UPDATE SET
                        first_cwd = excluded.first_cwd,
                        title = excluded.title,
                        started_at = excluded.started_at,
                        last_activity_at = excluded.last_activity_at,
                        active_time_ms = excluded.active_time_ms,
                        tokens_input = excluded.tokens_input,
                        tokens_output = excluded.tokens_output,
                        tokens_cache_write = excluded.tokens_cache_write,
                        tokens_cache_read = excluded.tokens_cache_read,
                        source_path = excluded.source_path",
                    params![
                        agent_id,
                        s.session_id,
                        s.first_cwd.as_str(),
                        s.title,
                        s.started_at,
                        s.last_activity_at,
                        s.active_time_ms,
                        s.tokens.input,
                        s.tokens.output,
                        s.tokens.cache_write,
                        s.tokens.cache_read,
                        path
                    ],
                )?;
            }
            tx.execute(
                "INSERT INTO source_file (path, agent_id, mtime, size) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (path) DO UPDATE SET agent_id = excluded.agent_id, mtime = excluded.mtime, size = excluded.size",
                params![path, agent_id, source.key.mtime, source.key.size as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn get_workspace(&self, id: i64) -> Result<Workspace, StoreError> {
        let mut ws = self
            .conn
            .query_row(
                "SELECT id, name, path, pinned, note, created_at FROM workspace WHERE id = ?1",
                [id],
                workspace_from_row,
            )
            .optional()?
            .ok_or(StoreError::NoSuchWorkspace(id))?;
        let mut stmt =
            self.conn.prepare("SELECT tag FROM workspace_tag WHERE workspace_id = ?1 ORDER BY position")?;
        ws.tags = stmt.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
        Ok(ws)
    }

    /// The Workspace that owns a path, by comparison key.
    fn path_owner(&self, key: &str) -> Result<Option<(i64, String)>, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT id, name FROM workspace WHERE path_key = ?1", [key], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?)
    }
}

fn insert_dismissed(conn: &Connection, path: &NormalizedPath) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO dismissed_path (path, path_key, dismissed_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (path_key) DO NOTHING",
        params![path.as_str(), path.comparison_key(), now_millis()],
    )?;
    Ok(())
}

/// `code-orchestrate.db` -> `code-orchestrate.db.bak`.
fn backup_path(db_path: &Path) -> PathBuf {
    let mut name = db_path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    db_path.with_file_name(name)
}

fn workspace_from_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        path: NormalizedPath::from_stored(row.get(2)?),
        pinned: row.get(3)?,
        note: row.get(4)?,
        created_at: row.get(5)?,
        tags: Vec::new(),
    })
}

/// The case-insensitive identity of a tag.
fn tag_key(tag: &str) -> String {
    tag.trim().to_lowercase()
}

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
