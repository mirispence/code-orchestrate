//! Missing and moved Workspaces (spec §6.2–§6.6): existence checks,
//! re-pointing, former paths and their ownership.

use code_orchestrate_lib::paths::{NormalizedPath, normalize};
use code_orchestrate_lib::store::Store;
use code_orchestrate_lib::workspaces::{self, AddWorkspaceError};

struct Dirs {
    root: tempfile::TempDir,
}

impl Dirs {
    fn new() -> Self {
        Dirs { root: tempfile::tempdir().unwrap() }
    }

    /// Create `name` under the temp root and return its raw path.
    fn make(&self, name: &str) -> String {
        let dir = self.root.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_str().unwrap().to_owned()
    }

    /// A path under the temp root that is not created.
    fn absent(&self, name: &str) -> String {
        self.root.path().join(name).to_str().unwrap().to_owned()
    }
}

fn norm(raw: &str) -> NormalizedPath {
    normalize(raw).unwrap()
}

#[test]
fn a_workspace_whose_folder_was_renamed_is_listed_as_missing() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let old = dirs.make("old");
    let ws = workspaces::add_workspace(&store, "App", &old).unwrap().workspace;
    assert!(!ws.missing);

    std::fs::rename(&old, dirs.absent("new")).unwrap();

    let listed = store.list_workspaces().unwrap();
    assert_eq!(listed.len(), 1, "a missing Workspace is never removed");
    assert!(listed[0].missing);
}

#[test]
fn re_pointing_keeps_the_old_path_as_a_former_path() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let old = dirs.make("old");
    let ws = workspaces::add_workspace(&store, "App", &old).unwrap().workspace;
    let new = dirs.make("new");

    let saved = workspaces::repoint_workspace(&store, ws.id, &new).unwrap();

    assert_eq!(saved.workspace.path, norm(&new));
    assert_eq!(saved.workspace.former_paths, vec![norm(&old)]);
    assert!(!saved.workspace.missing);
    assert_eq!(saved.moved_from, None);
    assert_eq!(store.list_workspaces().unwrap(), vec![saved.workspace]);
}

#[test]
fn re_pointing_requires_the_new_folder_to_exist_and_never_creates_it() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let ws = workspaces::add_workspace(&store, "App", &dirs.make("old")).unwrap().workspace;
    let absent = dirs.absent("nope");

    let err = workspaces::repoint_workspace(&store, ws.id, &absent).unwrap_err();

    assert!(matches!(err, AddWorkspaceError::NotFound { .. }), "{err:?}");
    assert!(!std::path::Path::new(&absent).exists());
    assert!(store.list_workspaces().unwrap()[0].former_paths.is_empty());
}

#[test]
fn re_pointing_onto_another_workspaces_current_path_is_rejected() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let a = workspaces::add_workspace(&store, "A", &dirs.make("a")).unwrap().workspace;
    let b = workspaces::add_workspace(&store, "B", &dirs.make("b")).unwrap().workspace;

    let err = workspaces::repoint_workspace(&store, a.id, b.path.as_str()).unwrap_err();

    assert_eq!(err, AddWorkspaceError::PathTaken { workspace_id: b.id, workspace_name: "B".into() });
}

#[test]
fn re_pointing_back_to_an_own_former_path_swaps_them() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let old = dirs.make("old");
    let new = dirs.make("new");
    let ws = workspaces::add_workspace(&store, "App", &old).unwrap().workspace;
    workspaces::repoint_workspace(&store, ws.id, &new).unwrap();

    let back = workspaces::repoint_workspace(&store, ws.id, &old).unwrap().workspace;

    assert_eq!(back.path, norm(&old));
    assert_eq!(back.former_paths, vec![norm(&new)]);
}

#[test]
fn re_pointing_to_the_same_path_changes_nothing() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let path = dirs.make("app");
    let ws = workspaces::add_workspace(&store, "App", &path).unwrap().workspace;

    let same = workspaces::repoint_workspace(&store, ws.id, &path).unwrap().workspace;

    assert_eq!(same, ws);
}

#[test]
fn adding_at_another_workspaces_former_path_moves_it_off_with_a_notice() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let old = dirs.make("old");
    let a = workspaces::add_workspace(&store, "A", &old).unwrap().workspace;
    workspaces::repoint_workspace(&store, a.id, &dirs.make("new")).unwrap();

    let added = workspaces::add_workspace(&store, "B", &old).unwrap();

    assert_eq!(added.workspace.path, norm(&old));
    let notice = added.moved_from.expect("a notice");
    assert_eq!((notice.workspace_id, notice.workspace_name.as_str()), (a.id, "A"));
    let a_now = store.list_workspaces().unwrap().into_iter().find(|w| w.id == a.id).unwrap();
    assert!(a_now.former_paths.is_empty());
}

#[test]
fn re_pointing_onto_another_workspaces_former_path_moves_it_off() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let old = dirs.make("old");
    let a = workspaces::add_workspace(&store, "A", &old).unwrap().workspace;
    workspaces::repoint_workspace(&store, a.id, &dirs.make("new")).unwrap();
    let b = workspaces::add_workspace(&store, "B", &dirs.make("b")).unwrap().workspace;

    let saved = workspaces::repoint_workspace(&store, b.id, &old).unwrap();

    assert_eq!(saved.workspace.path, norm(&old));
    assert_eq!(saved.moved_from.map(|m| m.workspace_id), Some(a.id));
    let a_now = store.list_workspaces().unwrap().into_iter().find(|w| w.id == a.id).unwrap();
    assert!(a_now.former_paths.is_empty());
}

#[test]
fn a_former_path_can_be_removed() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let old = dirs.make("old");
    let ws = workspaces::add_workspace(&store, "App", &old).unwrap().workspace;
    workspaces::repoint_workspace(&store, ws.id, &dirs.make("new")).unwrap();

    let after = workspaces::remove_former_path(&store, ws.id, &old).unwrap();

    assert!(after.former_paths.is_empty());
    // Free again: another Workspace can take it without a notice.
    assert_eq!(workspaces::add_workspace(&store, "B", &old).unwrap().moved_from, None);
}

#[test]
fn re_pointing_to_a_dismissed_path_restores_it() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let ws = workspaces::add_workspace(&store, "App", &dirs.make("old")).unwrap().workspace;
    let new = dirs.make("new");
    store.dismiss_path(&norm(&new)).unwrap();

    workspaces::repoint_workspace(&store, ws.id, &new).unwrap();

    assert!(store.list_dismissed_paths().unwrap().is_empty());
}

#[test]
fn removing_a_workspace_deletes_its_former_paths() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let old = dirs.make("old");
    let ws = workspaces::add_workspace(&store, "App", &old).unwrap().workspace;
    workspaces::repoint_workspace(&store, ws.id, &dirs.make("new")).unwrap();

    workspaces::remove_workspace(&store, ws.id).unwrap();

    assert_eq!(workspaces::add_workspace(&store, "B", &old).unwrap().moved_from, None);
}

#[test]
fn a_saved_workspace_serialises_with_its_notice() {
    let dirs = Dirs::new();
    let store = Store::open_in_memory().unwrap();
    let old = dirs.make("old");
    let a = workspaces::add_workspace(&store, "A", &old).unwrap().workspace;
    workspaces::repoint_workspace(&store, a.id, &dirs.make("new")).unwrap();

    let added = workspaces::add_workspace(&store, "B", &old).unwrap();
    let json = serde_json::to_value(&added).unwrap();

    assert_eq!(json["workspace"]["name"], "B");
    assert_eq!(json["workspace"]["formerPaths"], serde_json::json!([]));
    assert_eq!(json["workspace"]["missing"], false);
    assert_eq!(json["movedFrom"]["workspaceName"], "A");
}
