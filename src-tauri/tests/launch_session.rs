//! Launching a Session for a Workspace: adapter command, terminal wrapping,
//! spawn, and the errors the UI shows (spec §7.1–7.3).

use std::cell::RefCell;
use std::ffi::OsStr;
use std::io;

use code_orchestrate_lib::agents::LaunchMode;
use code_orchestrate_lib::agents::claude_code::ClaudeCode;
use code_orchestrate_lib::launch::{
    CustomTemplate, LaunchError, Spawner, Terminal, TerminalCommand, launch_session, test_terminal,
};
use code_orchestrate_lib::launch_env::{Environment, PermanentEnvironment, launch_environment};
use code_orchestrate_lib::store::Store;
use code_orchestrate_lib::workspaces::add_workspace;

/// Records what would have been spawned, optionally failing like the OS.
#[derive(Default)]
struct FakeSpawner {
    spawned: RefCell<Vec<TerminalCommand>>,
    fail_with: Option<io::ErrorKind>,
}

impl Spawner for FakeSpawner {
    fn spawn(&self, cmd: &TerminalCommand) -> io::Result<()> {
        if let Some(kind) = self.fail_with {
            return Err(io::Error::new(kind, "The system cannot find the file specified."));
        }
        self.spawned.borrow_mut().push(cmd.clone());
        Ok(())
    }
}

struct Fixture {
    store: Store,
    _workspace_dir: tempfile::TempDir,
    _bin: tempfile::TempDir,
    agent: ClaudeCode,
    workspace_id: i64,
    workspace_path: String,
    claude: String,
}

fn fixture() -> Fixture {
    let workspace_dir = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let exe = if cfg!(windows) { "claude.exe" } else { "claude" };
    let claude = bin.path().join(exe);
    std::fs::write(&claude, "").unwrap();
    let store = Store::open_in_memory().unwrap();
    let ws = add_workspace(&store, "App", workspace_dir.path().to_str().unwrap()).unwrap().workspace;
    Fixture {
        store,
        agent: ClaudeCode::with_search_path(vec![bin.path().to_path_buf()]),
        workspace_id: ws.id,
        workspace_path: ws.path.as_str().to_owned(),
        claude: claude.to_str().unwrap().to_owned(),
        _workspace_dir: workspace_dir,
        _bin: bin,
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| a.to_string()).collect()
}

#[test]
fn new_session_spawns_windows_terminal_running_claude_in_the_workspace() {
    let f = fixture();
    let spawner = FakeSpawner::default();

    launch_session(&f.store, &f.agent, Terminal::WindowsTerminalNewWindow, &spawner, &Vec::new(), f.workspace_id, &LaunchMode::New)
        .unwrap();

    assert_eq!(
        spawner.spawned.into_inner(),
        vec![TerminalCommand {
            program: "wt.exe".into(),
            args: strings(&["-w", "new", "new-tab", "-d", &f.workspace_path, &f.claude]),
            current_dir: None,
            env: Some(Vec::new()),
        }]
    );
}

#[test]
fn continue_last_spawns_claude_with_continue() {
    let f = fixture();
    let spawner = FakeSpawner::default();

    launch_session(&f.store, &f.agent, Terminal::WindowsTerminalNewWindow, &spawner, &Vec::new(), f.workspace_id, &LaunchMode::Continue)
        .unwrap();

    let spawned = spawner.spawned.into_inner();
    assert_eq!(spawned.len(), 1);
    assert_eq!(spawned[0].args[5..], strings(&[&f.claude, "--continue"]));
}

#[test]
fn a_failed_spawn_names_the_program_and_the_os_error() {
    let f = fixture();
    let spawner = FakeSpawner { fail_with: Some(io::ErrorKind::NotFound), ..Default::default() };

    let err = launch_session(&f.store, &f.agent, Terminal::WindowsTerminalNewWindow, &spawner, &Vec::new(), f.workspace_id, &LaunchMode::New)
        .unwrap_err();

    assert_eq!(
        err,
        LaunchError::SpawnFailed {
            program: "wt.exe".into(),
            os_error: "The system cannot find the file specified.".into(),
        }
    );
    assert_eq!(err.to_string(), "Could not start wt.exe: The system cannot find the file specified.");
}

#[test]
fn a_missing_executable_spawns_nothing() {
    let f = fixture();
    let empty = tempfile::tempdir().unwrap();
    let agent = ClaudeCode::with_search_path(vec![empty.path().to_path_buf()]);
    let spawner = FakeSpawner::default();

    let err = launch_session(&f.store, &agent, Terminal::WindowsTerminalNewWindow, &spawner, &Vec::new(), f.workspace_id, &LaunchMode::New)
        .unwrap_err();

    assert_eq!(err, LaunchError::ExecutableNotFound { name: "claude".into() });
    assert!(spawner.spawned.into_inner().is_empty());
}

#[test]
fn an_unknown_workspace_spawns_nothing() {
    let f = fixture();
    let spawner = FakeSpawner::default();

    let err = launch_session(&f.store, &f.agent, Terminal::WindowsTerminalNewWindow, &spawner, &Vec::new(), 999, &LaunchMode::New)
        .unwrap_err();

    assert_eq!(err, LaunchError::WorkspaceNotFound { workspace_id: 999 });
    assert!(spawner.spawned.into_inner().is_empty());
}

#[test]
fn launches_go_through_the_chosen_terminal() {
    let f = fixture();
    let spawner = FakeSpawner::default();

    launch_session(&f.store, &f.agent, Terminal::ConsoleWindow, &spawner, &Vec::new(), f.workspace_id, &LaunchMode::New).unwrap();

    let spawned = spawner.spawned.into_inner();
    assert_eq!(spawned[0].program, "cmd.exe");
    assert_eq!(spawned[0].args, strings(&["/c", "start", "", "/D", &f.workspace_path, &f.claude]));
}

#[test]
fn testing_a_terminal_runs_claude_version_in_the_home_directory_through_it() {
    let f = fixture();
    let spawner = FakeSpawner::default();
    let template = CustomTemplate::new(strings(&["alacritty", "--working-directory", "{cwd}", "-e", "{command}"])).unwrap();

    test_terminal(&f.agent, &Terminal::Custom(template), &spawner, &Vec::new(), r"C:\Users\me").unwrap();

    assert_eq!(
        spawner.spawned.into_inner(),
        vec![TerminalCommand {
            program: "alacritty".into(),
            args: strings(&["--working-directory", r"C:\Users\me", "-e", &f.claude, "--version"]),
            current_dir: Some(r"C:\Users\me".into()),
            env: Some(Vec::new()),
        }]
    );
}

struct NothingPermanent;

impl PermanentEnvironment for NothingPermanent {
    fn has(&self, _name: &OsStr) -> bool {
        false
    }
}

/// What a launch gets when npm started the app.
fn launch_env() -> Environment {
    let inherited = [("npm_lifecycle_event", "tauri"), ("CARGO_PKG_NAME", "app"), ("MY_OWN", "1")];
    launch_environment(inherited.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect(), &NothingPermanent)
}

fn only_my_own() -> Option<Environment> {
    Some(vec![("MY_OWN".into(), "1".into())])
}

#[test]
fn a_session_starts_with_the_launch_environment() {
    let f = fixture();
    let spawner = FakeSpawner::default();

    launch_session(&f.store, &f.agent, Terminal::ConsoleWindow, &spawner, &launch_env(), f.workspace_id, &LaunchMode::New)
        .unwrap();

    assert_eq!(spawner.spawned.into_inner()[0].env, only_my_own());
}

#[test]
fn testing_a_terminal_starts_it_with_the_launch_environment() {
    let f = fixture();
    let spawner = FakeSpawner::default();
    let template = CustomTemplate::new(strings(&["term", "{command}"])).unwrap();

    test_terminal(&f.agent, &Terminal::Custom(template), &spawner, &launch_env(), r"C:\Users\me").unwrap();

    assert_eq!(spawner.spawned.into_inner()[0].env, only_my_own());
}

#[test]
fn testing_a_terminal_reports_a_failed_spawn() {
    let f = fixture();
    let spawner = FakeSpawner { fail_with: Some(io::ErrorKind::NotFound), ..Default::default() };
    let template = CustomTemplate::new(strings(&["nope-term", "{command}"])).unwrap();

    let err = test_terminal(&f.agent, &Terminal::Custom(template), &spawner, &Vec::new(), r"C:\Users\me").unwrap_err();

    assert!(matches!(err, LaunchError::SpawnFailed { ref program, .. } if program == "nope-term"), "{err:?}");
}

#[test]
fn errors_reach_the_ui_as_kind_and_message() {
    let err = LaunchError::SpawnFailed { program: "wt.exe".into(), os_error: "Access is denied.".into() };

    assert_eq!(
        serde_json::to_value(&err).unwrap(),
        serde_json::json!({
            "kind": "spawnFailed",
            "message": "Could not start wt.exe: Access is denied.",
            "program": "wt.exe",
            "osError": "Access is denied.",
        })
    );
}

#[test]
fn a_missing_workspace_does_not_launch_and_its_folder_is_not_created() {
    let f = fixture();
    std::fs::remove_dir(&f.workspace_path).unwrap();
    let spawner = FakeSpawner::default();

    let err = launch_session(&f.store, &f.agent, Terminal::WindowsTerminalNewWindow, &spawner, &Vec::new(), f.workspace_id, &LaunchMode::New)
        .unwrap_err();

    assert_eq!(err, LaunchError::WorkspaceMissing { workspace_id: f.workspace_id, path: f.workspace_path.clone() });
    assert!(spawner.spawned.into_inner().is_empty());
    assert!(!std::path::Path::new(&f.workspace_path).exists());
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["kind"], "workspaceMissing");
    assert_eq!(json["workspaceId"], f.workspace_id);
}
