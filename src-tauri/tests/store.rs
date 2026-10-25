//! SQLite store: migrations, backup and Workspaces (spec §4).

use std::path::{Path, PathBuf};

use code_orchestrate_lib::paths::normalize;
use code_orchestrate_lib::store::{NewWorkspace, SCHEMA_VERSION, Store, StoreError, WorkspaceEdit};

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

// --- Editing, tags, pinning and removal (#18) ---

fn edit(name: &str, note: Option<&str>, tags: &[&str]) -> WorkspaceEdit {
    WorkspaceEdit {
        name: name.into(),
        note: note.map(Into::into),
        tags: tags.iter().map(|t| t.to_string()).collect(),
    }
}

#[test]
fn editing_changes_name_and_note_and_persists() {
    let f = fixture();
    let id = {
        let store = Store::open(&f.db).unwrap();
        let ws = store.add_workspace(new_ws("Code", CODE)).unwrap();
        let edited = store.update_workspace(ws.id, edit(" Renamed ", Some("my notes"), &[])).unwrap();
        assert_eq!(edited.name, "Renamed");
        assert_eq!(edited.note.as_deref(), Some("my notes"));
        ws.id
    };
    let reopened = Store::open(&f.db).unwrap();
    let ws = &reopened.list_workspaces().unwrap()[0];
    assert_eq!((ws.id, ws.name.as_str(), ws.note.as_deref()), (id, "Renamed", Some("my notes")));
}

#[test]
fn an_edit_with_an_empty_name_is_rejected_and_changes_nothing() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    let ws = store.add_workspace(new_ws("Code", CODE)).unwrap();
    assert!(matches!(store.update_workspace(ws.id, edit(" ", None, &["x"])), Err(StoreError::EmptyName)));
    assert_eq!(store.list_workspaces().unwrap(), vec![ws]);
}

#[test]
fn a_blank_note_is_stored_as_none() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    let ws = store.add_workspace(new_ws("Code", CODE)).unwrap();
    assert_eq!(store.update_workspace(ws.id, edit("Code", Some("  "), &[])).unwrap().note, None);
}

#[test]
fn tags_are_saved_and_listed_with_the_workspace() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    let ws = store.add_workspace(new_ws("Code", CODE)).unwrap();
    let edited = store.update_workspace(ws.id, edit("Code", None, &["rust", "Work"])).unwrap();
    assert_eq!(edited.tags, ["rust", "Work"]);
    assert_eq!(store.list_workspaces().unwrap()[0].tags, ["rust", "Work"]);
}

#[test]
fn tags_are_case_insensitive_within_a_workspace_keeping_the_first_spelling() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    let ws = store.add_workspace(new_ws("Code", CODE)).unwrap();
    let edited = store.update_workspace(ws.id, edit("Code", None, &["Rust", "rust ", " ", "RUST"])).unwrap();
    assert_eq!(edited.tags, ["Rust"]);
}

#[test]
fn a_tag_already_in_use_keeps_its_stored_spelling() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    let a = store.add_workspace(new_ws("A", CODE)).unwrap();
    let b = store.add_workspace(new_ws("B", OTHER)).unwrap();
    store.update_workspace(a.id, edit("A", None, &["Rust"])).unwrap();
    assert_eq!(store.update_workspace(b.id, edit("B", None, &["rust"])).unwrap().tags, ["Rust"]);
}

#[test]
fn tags_in_use_are_listed_and_a_tag_disappears_when_unused() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    let a = store.add_workspace(new_ws("A", CODE)).unwrap();
    let b = store.add_workspace(new_ws("B", OTHER)).unwrap();
    store.update_workspace(a.id, edit("A", None, &["work", "Rust"])).unwrap();
    store.update_workspace(b.id, edit("B", None, &["rust"])).unwrap();
    assert_eq!(store.list_tags().unwrap(), ["Rust", "work"]);

    store.update_workspace(a.id, edit("A", None, &[])).unwrap();
    assert_eq!(store.list_tags().unwrap(), ["Rust"]);
}

#[test]
fn pinning_and_unpinning_persist() {
    let f = fixture();
    let id = {
        let store = Store::open(&f.db).unwrap();
        let ws = store.add_workspace(new_ws("Code", CODE)).unwrap();
        assert!(store.set_pinned(ws.id, true).unwrap().pinned);
        ws.id
    };
    let store = Store::open(&f.db).unwrap();
    assert!(store.list_workspaces().unwrap()[0].pinned);
    assert!(!store.set_pinned(id, false).unwrap().pinned);
    assert!(!store.list_workspaces().unwrap()[0].pinned);
}

#[test]
fn removing_a_workspace_deletes_its_row_and_tags() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    let a = store.add_workspace(new_ws("A", CODE)).unwrap();
    let b = store.add_workspace(new_ws("B", OTHER)).unwrap();
    store.update_workspace(a.id, edit("A", None, &["gone", "shared"])).unwrap();
    store.update_workspace(b.id, edit("B", None, &["shared"])).unwrap();

    store.remove_workspace(a.id).unwrap();

    let names: Vec<_> = store.list_workspaces().unwrap().into_iter().map(|w| w.name).collect();
    assert_eq!(names, ["B"]);
    assert_eq!(store.list_tags().unwrap(), ["shared"]);
    // Its path is free again.
    store.add_workspace(new_ws("A again", CODE)).unwrap();
}

#[test]
fn removing_or_pinning_an_unknown_workspace_fails() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    assert!(matches!(store.remove_workspace(99), Err(StoreError::NoSuchWorkspace(99))));
    assert!(matches!(store.set_pinned(99, true), Err(StoreError::NoSuchWorkspace(99))));
}

#[test]
fn editing_an_unknown_workspace_fails() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    assert!(matches!(store.update_workspace(99, edit("X", None, &[])), Err(StoreError::NoSuchWorkspace(99))));
}

#[test]
fn one_workspace_is_read_by_id_as_listed() {
    let f = fixture();
    let store = Store::open(&f.db).unwrap();
    let ws = store.add_workspace(new_ws("A", CODE)).unwrap();
    store.update_workspace(ws.id, edit("A", Some("n"), &["t"])).unwrap();

    let listed = store.list_workspaces().unwrap().remove(0);
    assert_eq!(store.get_workspace(ws.id).unwrap(), listed);
    assert!(matches!(store.get_workspace(99), Err(StoreError::NoSuchWorkspace(99))));
}
