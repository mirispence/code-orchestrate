//! Resuming a past Session from a Workspace (spec §7.1 Resume, §8.2 Resume
//! table). Spawns go to a fake spawner, never a real process.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};

use code_orchestrate_lib::agents::claude_code::ClaudeCode;
use code_orchestrate_lib::agents::{LaunchMode, SessionSummary, SourceKey, TokenUsage};
use code_orchestrate_lib::attribution;
use code_orchestrate_lib::launch::{LaunchError, Spawner, Terminal, TerminalCommand, launch_session};
use code_orchestrate_lib::paths::normalize;
use code_orchestrate_lib::store::{ScannedSource, Store};
use code_orchestrate_lib::workspaces::add_workspace;

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

struct Fixture {
    store: Store,
    workspace_dir: tempfile::TempDir,
    transcripts: tempfile::TempDir,
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
        workspace_dir,
        transcripts: tempfile::tempdir().unwrap(),
        _bin: bin,
    }
}

impl Fixture {
    /// A subfolder of the Workspace, created on disk; returns its normalised path.
    fn subfolder(&self, name: &str) -> String {
        let path = self.workspace_dir.path().join(name);
        std::fs::create_dir_all(&path).unwrap();
        normalize(path.to_str().unwrap()).unwrap().as_str().to_owned()
    }

    /// Store a scanned Session `id` that started in `cwd`, with a transcript on disk.
    fn record(&self, id: &str, cwd: &str, last_activity_at: i64) -> PathBuf {
        let source = self.transcripts.path().join(format!("{id}.jsonl"));
        std::fs::write(&source, "{}\n").unwrap();
        let session = SessionSummary {
            session_id: id.to_owned(),
            first_cwd: normalize(cwd).unwrap(),
            title: Some(format!("Session {id}")),
            started_at: Some(last_activity_at - 1000),
            last_activity_at: Some(last_activity_at),
            active_time_ms: Some(1000),
            tokens: TokenUsage::default(),
            source_path: source.clone(),
        };
        let key = SourceKey { path: source.clone(), mtime: 1, size: 3 };
        self.store.record_scan("claude-code", &[ScannedSource { key, session: Some(session) }]).unwrap();
        source
    }

    fn resume(&self, spawner: &FakeSpawner, session_id: &str) -> Result<(), LaunchError> {
        launch_session(
            &self.store,
            &self.agent,
            Terminal::WindowsTerminalNewWindow,
            spawner,
            &Vec::new(),
            self.workspace_id,
            &LaunchMode::Resume(session_id.to_owned()),
        )
    }
}

fn wt_running(cwd: &str, args: &[&str]) -> TerminalCommand {
    let mut all: Vec<String> = ["-w", "new", "new-tab", "-d", cwd].iter().map(|s| s.to_string()).collect();
    all.extend(args.iter().map(|s| s.to_string()));
    TerminalCommand { program: "wt.exe".into(), args: all, current_dir: None, env: Some(Vec::new()) }
}

#[test]
fn resume_runs_claude_resume_in_the_sessions_first_cwd() {
    let f = fixture();
    let cwd = f.subfolder("server");
    f.record("abc-123", &cwd, 5_000);
    let spawner = FakeSpawner::default();

    f.resume(&spawner, "abc-123").unwrap();

    assert_eq!(spawner.spawned.into_inner(), vec![wt_running(&cwd, &[&f.claude, "--resume", "abc-123"])]);
}

#[test]
fn resume_falls_back_to_the_workspace_path_when_first_cwd_is_gone() {
    let f = fixture();
    let cwd = f.subfolder("old-module");
    f.record("abc-123", &cwd, 5_000);
    std::fs::remove_dir(&cwd).unwrap();
    let spawner = FakeSpawner::default();

    f.resume(&spawner, "abc-123").unwrap();

    assert_eq!(
        spawner.spawned.into_inner(),
        vec![wt_running(&f.workspace_path, &[&f.claude, "--resume", "abc-123"])]
    );
}

#[test]
fn a_session_whose_transcript_was_deleted_keeps_its_stats_but_is_marked() {
    let f = fixture();
    let cwd = f.subfolder("server");
    let kept = f.record("kept", &cwd, 6_000);
    let deleted = f.record("deleted", &cwd, 5_000);
    std::fs::remove_file(&deleted).unwrap();

    let sessions = attribution::workspace_sessions(&f.store, f.workspace_id).unwrap();

    let summary: Vec<(&str, bool, Option<i64>)> =
        sessions.iter().map(|s| (s.session_id.as_str(), s.transcript_exists, s.active_time_ms)).collect();
    assert_eq!(summary, vec![("kept", true, Some(1000)), ("deleted", false, Some(1000))]);
    assert!(Path::new(&kept).exists());
    let json = serde_json::to_value(&sessions[1]).unwrap();
    assert_eq!(json["transcriptExists"], false);
}

#[test]
fn resuming_a_session_whose_transcript_was_deleted_spawns_nothing() {
    let f = fixture();
    let cwd = f.subfolder("server");
    let source = f.record("abc-123", &cwd, 5_000);
    std::fs::remove_file(&source).unwrap();
    let spawner = FakeSpawner::default();

    let err = f.resume(&spawner, "abc-123").unwrap_err();

    assert_eq!(err, LaunchError::TranscriptDeleted { session_id: "abc-123".into() });
    assert!(spawner.spawned.into_inner().is_empty());
    assert_eq!(serde_json::to_value(&err).unwrap()["kind"], "transcriptDeleted");
}

#[test]
fn the_ui_sends_the_launch_mode_as_new_continue_or_resume_with_the_session_id() {
    let mode = |v| serde_json::from_value::<LaunchMode>(v).unwrap();
    assert_eq!(mode(serde_json::json!("new")), LaunchMode::New);
    assert_eq!(mode(serde_json::json!("continue")), LaunchMode::Continue);
    assert_eq!(mode(serde_json::json!({"resume": "abc-123"})), LaunchMode::Resume("abc-123".into()));
}

#[test]
fn resuming_a_session_of_another_workspace_spawns_nothing() {
    let f = fixture();
    let elsewhere = tempfile::tempdir().unwrap();
    f.record("other-1", elsewhere.path().to_str().unwrap(), 5_000);
    let spawner = FakeSpawner::default();

    let err = f.resume(&spawner, "other-1").unwrap_err();

    assert_eq!(err, LaunchError::SessionNotFound { session_id: "other-1".into() });
    assert!(spawner.spawned.into_inner().is_empty());
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["kind"], "sessionNotFound");
    assert_eq!(json["sessionId"], "other-1");
}
