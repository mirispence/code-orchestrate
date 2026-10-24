//! SQLite access and migrations (spec §4). The rest of the app sees typed
//! records and domain errors; SQL stays in this module.
//!
//! Adding a table or column: append a migration to [`MIGRATIONS`] (never edit
//! a shipped one). Path uniqueness across Workspace current (and, later,
//! former) paths is enforced here by each row's `path_key`, the path's
//! [`NormalizedPath::comparison_key`].

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

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
];

/// The schema version this build migrates to.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("A Workspace needs a name")]
    EmptyName,
    #[error("Already a Workspace: {workspace_name}")]
    PathTaken { workspace_id: i64, workspace_name: String },
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
}

#[derive(Debug, Clone)]
pub struct NewWorkspace {
    pub name: String,
    pub path: NormalizedPath,
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
        let rows = stmt.query_map([], workspace_from_row)?;
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
        self.conn.execute(
            "INSERT INTO workspace (name, path, path_key, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![name, new.path.as_str(), key, now_millis()],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(self.conn.query_row(
            "SELECT id, name, path, pinned, note, created_at FROM workspace WHERE id = ?1",
            [id],
            workspace_from_row,
        )?)
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
    })
}

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
