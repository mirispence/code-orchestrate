//! SQLite store: migrations, backup and Workspaces (spec §4).

use std::path::{Path, PathBuf};

use code_orchestrate_lib::paths::normalize;
use code_orchestrate_lib::store::{NewWorkspace, SCHEMA_VERSION, Store, StoreError};

struct Fixture {
    _dir: tempfile::TempDir,
    db: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("code-orchestrate.db");
    Fixture { _dir: dir, db }
}

fn bak(db: &Path) -> PathBuf {
    db.with_file_name("code-orchestrate.db.bak")
}

fn new_ws(name: &str, path: &str) -> NewWorkspace {
    NewWorkspace { name: name.into(), path: normalize(path).unwrap() }
}

const CODE: &str = if cfg!(windows) { r"E:\code" } else { "/code" };
const OTHER: &str = if cfg!(windows) { r"E:\other" } else { "/other" };

#[test]
fn a_new_store_has_no_workspaces() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    assert!(store.list_workspaces().unwrap().is_empty());
}

#[test]
fn added_workspaces_are_listed_with_their_fields() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    let added = store.add_workspace(new_ws("Code", CODE)).unwrap();

    let listed = store.list_workspaces().unwrap();
    assert_eq!(listed, vec![added.clone()]);
    assert_eq!(added.name, "Code");
    assert_eq!(added.path.as_str(), CODE);
    assert!(!added.pinned);
    assert_eq!(added.note, None);
    assert!(added.created_at > 0);
}

#[test]
fn workspaces_survive_reopening_the_store() {
    let f = fixture();
    {
        let store = Store::open(&f.db).unwrap();
        store.add_workspace(new_ws("Code", CODE)).unwrap();
    }
    let reopened = Store::open(&f.db).unwrap();
    let names: Vec<_> = reopened.list_workspaces().unwrap().into_iter().map(|w| w.name).collect();
    assert_eq!(names, ["Code"]);
}

#[test]
fn workspaces_are_listed_alphabetically_ignoring_case() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    store.add_workspace(new_ws("beta", CODE)).unwrap();
    store.add_workspace(new_ws("Alpha", OTHER)).unwrap();
    let names: Vec<_> = store.list_workspaces().unwrap().into_iter().map(|w| w.name).collect();
    assert_eq!(names, ["Alpha", "beta"]);
}

#[test]
fn names_need_not_be_unique() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    store.add_workspace(new_ws("Same", CODE)).unwrap();
    store.add_workspace(new_ws("Same", OTHER)).unwrap();
    assert_eq!(store.list_workspaces().unwrap().len(), 2);
}

#[test]
fn an_empty_name_is_rejected() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    assert!(matches!(store.add_workspace(new_ws("  ", CODE)), Err(StoreError::EmptyName)));
}

#[test]
fn names_are_trimmed() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    assert_eq!(store.add_workspace(new_ws("  Code ", CODE)).unwrap().name, "Code");
}

#[test]
fn a_path_already_used_by_a_workspace_is_rejected_naming_it() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    let first = store.add_workspace(new_ws("Code", CODE)).unwrap();

    let same_spelling_variant = if cfg!(windows) { r"e:\CODE\" } else { "/code/" };
    match store.add_workspace(new_ws("Again", same_spelling_variant)) {
        Err(StoreError::PathTaken { workspace_id, workspace_name }) => {
            assert_eq!(workspace_id, first.id);
            assert_eq!(workspace_name, "Code");
        }
        other => panic!("expected PathTaken, got {other:?}"),
    }
    assert_eq!(store.list_workspaces().unwrap().len(), 1);
}

#[test]
fn nested_workspaces_are_allowed() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    store.add_workspace(new_ws("Code", CODE)).unwrap();
    let nested = if cfg!(windows) { r"E:\code\inner" } else { "/code/inner" };
    store.add_workspace(new_ws("Inner", nested)).unwrap();
    assert_eq!(store.list_workspaces().unwrap().len(), 2);
}

#[test]
fn a_fresh_store_is_at_the_latest_schema_version() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
}

#[test]
fn migrating_backs_up_the_database_first() {
    let f = fixture();
    // A database from before any migration, holding data of its own.
    {
        let conn = rusqlite::Connection::open(&f.db).unwrap();
        conn.execute_batch("CREATE TABLE legacy (x); INSERT INTO legacy VALUES (42);").unwrap();
    }
    std::fs::write(bak(&f.db), b"an older backup").unwrap();

    Store::open(&f.db).unwrap();

    let backup = rusqlite::Connection::open(bak(&f.db)).unwrap();
    let version: i64 = backup.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    let x: i64 = backup.query_row("SELECT x FROM legacy", [], |r| r.get(0)).unwrap();
    assert_eq!((version, x), (0, 42), "the backup is the pre-migration database");
}

#[test]
fn opening_an_up_to_date_database_makes_no_backup() {
    let f = fixture();
    Store::open(&f.db).unwrap();
    std::fs::remove_file(bak(&f.db)).unwrap();

    Store::open(&f.db).unwrap();
    assert!(!bak(&f.db).exists());
}

#[test]
fn a_database_from_a_newer_app_is_refused() {
    let f = fixture();
    {
        let conn = rusqlite::Connection::open(&f.db).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1).unwrap();
    }
    assert!(matches!(Store::open(&f.db), Err(StoreError::NewerSchema { .. })));
}

#[test]
fn the_database_file_is_code_orchestrate_db() {
    assert_eq!(code_orchestrate_lib::DB_FILE_NAME, "code-orchestrate.db");
}
