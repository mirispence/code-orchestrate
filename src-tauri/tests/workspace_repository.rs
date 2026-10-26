//! A Workspace's git repository (#32): its remote's web page, shown in the
//! detail header and opened in the default browser.

use std::cell::RefCell;
use std::io;
use std::path::Path;

use code_orchestrate_lib::launch::{Spawner, TerminalCommand};
use code_orchestrate_lib::repository::{
    Git, OsGit, Repository, RepositoryError, find, get_workspace, open_workspace_repository, workspace_repository,
};
use code_orchestrate_lib::store::{Store, Workspace};
use code_orchestrate_lib::workspaces::add_workspace;

/// Answers `git remote -v` with `stdout`; `None` fails like a folder outside
/// any repository.
struct FakeGit(Option<&'static str>);

impl Git for FakeGit {
    fn run(&self, _dir: &Path, args: &[&str]) -> Option<String> {
        assert_eq!(args, ["remote", "-v"]);
        self.0.map(str::to_owned)
    }
}

const ORIGIN: FakeGit = FakeGit(Some(
    "origin\tgit@github.com:owner/repo.git (fetch)\norigin\tgit@github.com:owner/repo.git (push)\n",
));

/// Fails the test if git runs at all.
struct NoGit;

impl Git for NoGit {
    fn run(&self, _dir: &Path, _args: &[&str]) -> Option<String> {
        panic!("git must not run for a Missing Workspace");
    }
}

#[derive(Default)]
struct FakeSpawner {
    spawned: RefCell<Vec<TerminalCommand>>,
}

impl Spawner for FakeSpawner {
    fn spawn(&self, cmd: &TerminalCommand) -> io::Result<()> {
        self.spawned.borrow_mut().push(cmd.clone());
        Ok(())
    }
}

/// The Workspace read back as the commands read it (`missing` checked now).
fn read(store: &Store, id: i64) -> Workspace {
    get_workspace(store, id).unwrap()
}

fn add(store: &Store, dir: &tempfile::TempDir) -> i64 {
    add_workspace(store, "App", dir.path().to_str().unwrap()).unwrap().workspace.id
}

fn opener() -> &'static str {
    if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

#[test]
fn a_workspace_with_an_origin_shows_its_repository() {
    let store = Store::open_in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let id = add(&store, &dir);

    assert_eq!(
        workspace_repository(&read(&store, id), &ORIGIN),
        Some(Repository { name: "owner/repo".into(), url: "https://github.com/owner/repo".into() })
    );
}

#[test]
fn a_missing_workspace_shows_no_repository_and_runs_no_git() {
    let store = Store::open_in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let id = add(&store, &dir);
    drop(dir);

    assert_eq!(workspace_repository(&read(&store, id), &NoGit), None);
}

#[test]
fn opening_re_resolves_the_page_and_hands_it_to_the_system_opener() {
    let store = Store::open_in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let id = add(&store, &dir);
    let spawner = FakeSpawner::default();

    open_workspace_repository(&read(&store, id), &ORIGIN, &spawner).unwrap();

    assert_eq!(
        spawner.spawned.into_inner(),
        vec![TerminalCommand {
            program: opener().into(),
            args: vec!["https://github.com/owner/repo".into()],
            current_dir: None,
        }]
    );
}

#[test]
fn opening_without_a_repository_is_an_error_and_opens_nothing() {
    let store = Store::open_in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let id = add(&store, &dir);
    let spawner = FakeSpawner::default();

    let error = open_workspace_repository(&read(&store, id), &FakeGit(None), &spawner).unwrap_err();

    assert_eq!(error, RepositoryError::NoRepository);
    assert!(spawner.spawned.into_inner().is_empty());
}

#[test]
fn opening_a_missing_workspace_is_an_error_and_runs_nothing() {
    let store = Store::open_in_memory().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let id = add(&store, &dir);
    drop(dir);
    let spawner = FakeSpawner::default();

    let error = open_workspace_repository(&read(&store, id), &NoGit, &spawner).unwrap_err();

    assert!(matches!(error, RepositoryError::WorkspaceMissing { workspace_id, .. } if workspace_id == id));
    assert!(spawner.spawned.into_inner().is_empty());
}

#[test]
fn an_unknown_workspace_is_an_error() {
    let store = Store::open_in_memory().unwrap();

    assert_eq!(get_workspace(&store, 99).unwrap_err(), RepositoryError::WorkspaceNotFound { workspace_id: 99 });
}

/// The real `git`, in a repository and a subfolder of it and outside any.
/// Skipped when git is not on PATH.
#[test]
fn os_git_reads_the_origin_from_any_folder_in_the_repository() {
    let git = |dir: &Path, args: &[&str]| std::process::Command::new("git").args(args).current_dir(dir).output();
    let dir = tempfile::tempdir().unwrap();
    if git(dir.path(), &["init", "-q"]).is_err() {
        return;
    }
    git(dir.path(), &["remote", "add", "origin", "git@github.com:owner/repo.git"]).unwrap();
    let sub = dir.path().join("src");
    std::fs::create_dir(&sub).unwrap();

    let expected = Some(Repository { name: "owner/repo".into(), url: "https://github.com/owner/repo".into() });
    assert_eq!(find(&OsGit, dir.path()), expected);
    assert_eq!(find(&OsGit, &sub), expected);

    let outside = tempfile::tempdir().unwrap();
    assert_eq!(find(&OsGit, outside.path()), None);
}
