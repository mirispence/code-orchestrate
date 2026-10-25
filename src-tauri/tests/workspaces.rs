//! Adding a Workspace from a raw path (spec §6.2).

use code_orchestrate_lib::store::{Store, WorkspaceEdit};
use code_orchestrate_lib::workspaces::{
    AddWorkspaceError, WorkspaceError, add_workspace, remove_workspace, set_pinned, update_workspace,
};

#[test]
fn adds_an_existing_directory_under_its_normalised_path() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let raw = format!("{}{}", dir.path().display(), std::path::MAIN_SEPARATOR);

    let ws = add_workspace(&store, "Temp", &raw).unwrap().workspace;

    let expected = code_orchestrate_lib::paths::normalize(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(ws.path, expected);
    assert_eq!(store.list_workspaces().unwrap(), vec![ws]);
}

#[test]
fn rejects_a_path_that_does_not_exist() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let missing = dir.path().join("nope");

    let err = add_workspace(&store, "Nope", missing.to_str().unwrap()).unwrap_err();
    assert!(matches!(err, AddWorkspaceError::NotFound { .. }), "{err:?}");
    assert!(store.list_workspaces().unwrap().is_empty());
}

#[test]
fn rejects_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, "x").unwrap();
    let store = Store::open_in_memory().unwrap();

    let err = add_workspace(&store, "File", file.to_str().unwrap()).unwrap_err();
    assert!(matches!(err, AddWorkspaceError::NotADirectory { .. }), "{err:?}");
}

#[test]
fn a_duplicate_reports_the_owning_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let first = add_workspace(&store, "First", dir.path().to_str().unwrap()).unwrap().workspace;

    let variant = format!("{}{}", dir.path().display().to_string().to_uppercase(), std::path::MAIN_SEPARATOR);
    let variant = if cfg!(windows) { variant } else { format!("{}/", dir.path().display()) };
    let err = add_workspace(&store, "Second", &variant).unwrap_err();

    assert_eq!(
        err,
        AddWorkspaceError::PathTaken { workspace_id: first.id, workspace_name: "First".into() }
    );
    assert_eq!(err.to_string(), "Already a Workspace: First");
}

#[test]
fn errors_serialise_for_the_ui_with_a_kind_and_message() {
    let err = AddWorkspaceError::PathTaken { workspace_id: 7, workspace_name: "Code".into() };
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["kind"], "pathTaken");
    assert_eq!(json["workspaceId"], 7);
    assert_eq!(json["workspaceName"], "Code");
    assert_eq!(json["message"], "Already a Workspace: Code");
}

#[test]
fn edit_errors_serialise_for_the_ui() {
    let store = Store::open_in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ws = add_workspace(&store, "W", dir.path().to_str().unwrap()).unwrap().workspace;

    let blank = WorkspaceEdit { name: " ".into(), note: None, tags: vec![] };
    let err = update_workspace(&store, ws.id, blank).unwrap_err();
    assert_eq!(err, WorkspaceError::EmptyName);
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!((json["kind"].as_str(), json["message"].as_str()), (Some("emptyName"), Some("A Workspace needs a name")));

    let err = remove_workspace(&store, 999).unwrap_err();
    assert_eq!(serde_json::to_value(&err).unwrap()["kind"], "noSuchWorkspace");
    assert_eq!(set_pinned(&store, 999, true).unwrap_err(), WorkspaceError::NoSuchWorkspace);
}

#[test]
fn edits_pins_and_removes_through_the_workspace_ops() {
    let store = Store::open_in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ws = add_workspace(&store, "W", dir.path().to_str().unwrap()).unwrap().workspace;

    let edited = update_workspace(
        &store,
        ws.id,
        WorkspaceEdit { name: "Renamed".into(), note: Some("n".into()), tags: vec!["t".into()] },
    )
    .unwrap();
    assert_eq!((edited.name.as_str(), edited.tags.clone()), ("Renamed", vec!["t".to_string()]));
    assert!(set_pinned(&store, ws.id, true).unwrap().pinned);
    remove_workspace(&store, ws.id).unwrap();
    assert!(store.list_workspaces().unwrap().is_empty());
}

#[test]
fn rejects_an_empty_name() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let err = add_workspace(&store, "", dir.path().to_str().unwrap()).unwrap_err();
    assert_eq!(err, AddWorkspaceError::EmptyName);
}
