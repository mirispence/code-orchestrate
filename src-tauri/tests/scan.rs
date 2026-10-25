//! Scanning transcripts into durable Session records, and the Suggestions
//! computed from them on read (spec §4.3, §5.1, §5.4, §5.5, §6.6). Runs
//! against fixture transcript roots, never the user's real `~/.claude`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use code_orchestrate_lib::agents::claude_code::ClaudeCode;
use code_orchestrate_lib::agents::{
    AgentAdapter, Capabilities, LaunchCommand, LaunchCommandError, LaunchMode, SessionSummary, SourceKey,
};
use code_orchestrate_lib::attribution::{self, Suggestion};
use code_orchestrate_lib::paths::{NormalizedPath, normalize};
use code_orchestrate_lib::scan;
use code_orchestrate_lib::store::{NewWorkspace, Store};

struct Fixture {
    root: tempfile::TempDir,
    /// Real directories Sessions run in.
    dirs: tempfile::TempDir,
    store: Mutex<Store>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("projects")).unwrap();
        Fixture { root, dirs: tempfile::tempdir().unwrap(), store: Mutex::new(Store::open_in_memory().unwrap()) }
    }

    fn agent(&self) -> ClaudeCode {
        ClaudeCode::on_path().with_transcript_root(self.root.path().to_path_buf())
    }

    /// A real directory named `name` (may contain separators).
    fn dir(&self, name: &str) -> NormalizedPath {
        let path = self.dirs.path().join(name);
        fs::create_dir_all(&path).unwrap();
        normalize(path.to_str().unwrap()).unwrap()
    }

    /// A transcript in `folder` whose records ran in `cwd` at these times.
    fn transcript(&self, folder: &str, session_id: &str, cwd: &str, times: &[&str]) -> PathBuf {
        let dir = self.root.path().join("projects").join(folder);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{session_id}.jsonl"));
        let body: String = times
            .iter()
            .map(|t| format!("{}\n", serde_json::json!({"type": "user", "cwd": cwd, "timestamp": t})))
            .collect();
        fs::write(&path, body).unwrap();
        path
    }

    fn scan(&self) -> scan::ScanReport {
        scan::run(&self.store, &self.agent()).unwrap()
    }

    fn suggestions(&self) -> Vec<Suggestion> {
        attribution::suggestions(&self.store.lock().unwrap()).unwrap()
    }

    fn add_workspace(&self, path: &NormalizedPath) -> i64 {
        let store = self.store.lock().unwrap();
        store.add_workspace(NewWorkspace { name: "w".into(), path: path.clone() }).unwrap().id
    }
}

fn paths(suggestions: &[Suggestion]) -> Vec<String> {
    suggestions.iter().map(|s| s.path.to_string()).collect()
}

#[test]
fn directories_with_history_become_suggestions_with_counts_and_last_seen() {
    let f = Fixture::new();
    let app = f.dir("app");
    f.transcript("enc-app", "s1", app.as_str(), &["2026-10-24T04:00:00Z", "2026-10-24T05:00:00Z"]);
    f.transcript("enc-app", "s2", app.as_str(), &["2026-10-20T04:00:00Z"]);

    let report = f.scan();

    assert_eq!(report.parsed, 2);
    assert_eq!(
        f.suggestions(),
        vec![Suggestion { path: app, session_count: 2, last_seen: Some(1_792_818_000_000), missing: false }]
    );
}

#[test]
fn colliding_folder_names_still_give_each_real_path() {
    let f = Fixture::new();
    let dash = f.dir("a-b");
    let under = f.dir("a_b");
    // Claude Code encodes both as the same folder name.
    f.transcript("X--a-b", "s1", dash.as_str(), &["2026-10-24T04:00:00Z"]);
    f.transcript("X--a-b", "s2", under.as_str(), &["2026-10-24T04:00:00Z"]);

    f.scan();

    let mut expected = vec![dash.to_string(), under.to_string()];
    expected.sort();
    let mut got = paths(&f.suggestions());
    got.sort();
    assert_eq!(got, expected);
}

#[test]
fn a_second_scan_with_no_changes_parses_nothing() {
    let f = Fixture::new();
    let app = f.dir("app");
    f.transcript("p", "s1", app.as_str(), &["2026-10-24T04:00:00Z"]);
    f.scan();

    let report = f.scan();

    assert_eq!(report.sources, 1);
    assert_eq!(report.parsed, 0);
}

#[test]
fn a_changed_transcript_is_parsed_again() {
    let f = Fixture::new();
    let app = f.dir("app");
    f.transcript("p", "s1", app.as_str(), &["2026-10-24T04:00:00Z"]);
    f.scan();
    f.transcript("p", "s1", app.as_str(), &["2026-10-24T04:00:00Z", "2026-10-25T04:00:00Z"]);

    let report = f.scan();

    assert_eq!(report.parsed, 1);
    assert_eq!(f.suggestions()[0].last_seen, Some(1_792_900_800_000));
    assert_eq!(f.suggestions()[0].session_count, 1);
}

/// Delegates to Claude Code but fails to read every transcript while `locked`.
struct FlakyReads {
    inner: ClaudeCode,
    locked: AtomicBool,
}

impl AgentAdapter for FlakyReads {
    fn id(&self) -> &'static str {
        self.inner.id()
    }
    fn display_name(&self) -> &'static str {
        self.inner.display_name()
    }
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn resolve_executable(&self) -> Option<String> {
        None
    }
    fn launch_command(&self, p: &NormalizedPath, m: &LaunchMode) -> Result<LaunchCommand, LaunchCommandError> {
        self.inner.launch_command(p, m)
    }
    fn version_command(&self, cwd: &str) -> Result<LaunchCommand, LaunchCommandError> {
        self.inner.version_command(cwd)
    }
    fn list_sources(&self) -> Vec<SourceKey> {
        self.inner.list_sources()
    }
    fn parse_source(&self, path: &Path) -> std::io::Result<Option<SessionSummary>> {
        if self.locked.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "locked"));
        }
        self.inner.parse_source(path)
    }
}

#[test]
fn a_transcript_that_could_not_be_read_is_retried_next_scan() {
    let f = Fixture::new();
    let app = f.dir("app");
    f.transcript("p", "s1", app.as_str(), &["2026-10-24T04:00:00Z"]);
    let agent = FlakyReads { inner: f.agent(), locked: AtomicBool::new(true) };
    scan::run(&f.store, &agent).unwrap();
    assert!(f.suggestions().is_empty());

    agent.locked.store(false, Ordering::SeqCst);
    let report = scan::run(&f.store, &agent).unwrap();

    assert_eq!(report.parsed, 1);
    assert_eq!(paths(&f.suggestions()), vec![app.to_string()]);
}

#[test]
fn a_deleted_transcript_keeps_its_session() {
    let f = Fixture::new();
    let app = f.dir("app");
    let path = f.transcript("p", "s1", app.as_str(), &["2026-10-24T04:00:00Z"]);
    f.scan();
    fs::remove_file(path).unwrap();

    f.scan();

    assert_eq!(f.suggestions().len(), 1);
    assert_eq!(f.store.lock().unwrap().list_sessions().unwrap().len(), 1);
}

#[test]
fn a_session_inside_a_workspace_is_not_a_suggestion() {
    let f = Fixture::new();
    let app = f.dir("app");
    let sub = f.dir("app/sub");
    let other = f.dir("other");
    f.transcript("p", "s1", sub.as_str(), &["2026-10-24T04:00:00Z"]);
    f.transcript("p", "s2", other.as_str(), &["2026-10-24T04:00:00Z"]);
    f.add_workspace(&app);

    f.scan();

    assert_eq!(paths(&f.suggestions()), vec![other.to_string()]);
}

#[test]
fn the_deepest_matching_workspace_owns_a_session() {
    let f = Fixture::new();
    let outer = f.dir("code");
    let inner = f.dir("code/app");
    let sibling = f.dir("code-app");
    let outer_id = f.add_workspace(&outer);
    let inner_id = f.add_workspace(&inner);
    let workspaces = f.store.lock().unwrap().list_workspaces().unwrap();

    let owner = |p: &NormalizedPath| attribution::owner(p, &workspaces).map(|w| w.id);
    assert_eq!(owner(&f.dir("code/app/src")), Some(inner_id));
    assert_eq!(owner(&inner), Some(inner_id));
    assert_eq!(owner(&f.dir("code/lib")), Some(outer_id));
    assert_eq!(owner(&sibling), None);
}

#[test]
fn a_dismissed_suggestion_stays_hidden_after_new_sessions() {
    let f = Fixture::new();
    let app = f.dir("app");
    f.transcript("p", "s1", app.as_str(), &["2026-10-24T04:00:00Z"]);
    f.scan();

    f.store.lock().unwrap().dismiss_path(&app).unwrap();
    f.transcript("p", "s2", app.as_str(), &["2026-10-25T04:00:00Z"]);
    f.scan();

    assert!(f.suggestions().is_empty());
}

#[test]
fn a_suggestion_whose_directory_is_gone_is_missing() {
    let f = Fixture::new();
    let gone = f.dir("gone");
    f.transcript("p", "s1", gone.as_str(), &["2026-10-24T04:00:00Z"]);
    fs::remove_dir(gone.as_path()).unwrap();

    f.scan();

    assert!(f.suggestions()[0].missing);
}

#[test]
fn removing_a_workspace_dismisses_its_path_and_keeps_its_sessions() {
    let f = Fixture::new();
    let app = f.dir("app");
    f.transcript("p", "s1", app.as_str(), &["2026-10-24T04:00:00Z"]);
    let id = f.add_workspace(&app);
    f.scan();

    f.store.lock().unwrap().remove_workspace(id).unwrap();

    assert!(f.suggestions().is_empty());
    let store = f.store.lock().unwrap();
    assert_eq!(store.list_sessions().unwrap().len(), 1);
    assert_eq!(store.list_dismissed_paths().unwrap(), vec![app]);
}

#[test]
fn adding_a_workspace_at_a_dismissed_path_restores_it() {
    let f = Fixture::new();
    let app = f.dir("app");
    f.store.lock().unwrap().dismiss_path(&app).unwrap();

    f.add_workspace(&app);

    assert!(f.store.lock().unwrap().list_dismissed_paths().unwrap().is_empty());
}

#[test]
fn a_restored_path_can_reappear_as_a_suggestion() {
    let f = Fixture::new();
    let app = f.dir("app");
    f.transcript("p", "s1", app.as_str(), &["2026-10-24T04:00:00Z"]);
    f.scan();
    f.store.lock().unwrap().dismiss_path(&app).unwrap();

    f.store.lock().unwrap().restore_dismissed_path(&app).unwrap();

    assert!(f.store.lock().unwrap().list_dismissed_paths().unwrap().is_empty());
    assert_eq!(paths(&f.suggestions()), vec![app.to_string()]);
}

#[test]
fn a_rescan_parses_every_file_on_disk_regardless_of_source_key() {
    let f = Fixture::new();
    let app = f.dir("app");
    f.transcript("p", "s1", app.as_str(), &["2026-10-24T04:00:00Z"]);
    f.transcript("p", "s2", app.as_str(), &["2026-10-24T05:00:00Z"]);
    f.scan();

    let report = scan::run_full(&f.store, &f.agent()).unwrap();

    assert_eq!(report.parsed, 2);
    assert_eq!(f.scan().parsed, 0, "an ordinary scan still skips unchanged files");
}

#[test]
fn a_rescan_keeps_sessions_whose_transcript_is_gone() {
    let f = Fixture::new();
    let app = f.dir("app");
    let path = f.transcript("p", "s1", app.as_str(), &["2026-10-24T04:00:00Z"]);
    f.scan();
    fs::remove_file(path).unwrap();

    scan::run_full(&f.store, &f.agent()).unwrap();

    assert_eq!(f.store.lock().unwrap().list_sessions().unwrap().len(), 1);
}

#[test]
fn changing_the_transcript_root_keeps_sessions_and_reads_the_new_root() {
    let f = Fixture::new();
    let old = f.dir("old");
    let new = f.dir("new");
    f.transcript("p", "s1", old.as_str(), &["2026-10-24T04:00:00Z"]);
    f.scan();

    let other = tempfile::tempdir().unwrap();
    let dir = other.path().join("projects").join("q");
    fs::create_dir_all(&dir).unwrap();
    let line = serde_json::json!({"type": "user", "cwd": new.as_str(), "timestamp": "2026-10-25T04:00:00Z"});
    fs::write(dir.join("s2.jsonl"), format!("{line}\n")).unwrap();
    let moved = ClaudeCode::on_path().with_transcript_root(other.path().to_path_buf());
    scan::run(&f.store, &moved).unwrap();

    let mut found = paths(&f.suggestions());
    found.sort();
    let mut expected = vec![old.to_string(), new.to_string()];
    expected.sort();
    assert_eq!(found, expected);
}

#[test]
fn sessions_at_a_former_path_keep_attributing_after_re_pointing() {
    let f = Fixture::new();
    let old = f.dir("old");
    let new = f.dir("new");
    f.transcript("p", "s1", old.as_str(), &["2026-10-24T04:00:00Z"]);
    f.transcript("p", "s2", new.as_str(), &["2026-10-25T04:00:00Z"]);
    let id = f.add_workspace(&old);
    f.store.lock().unwrap().repoint_workspace(id, &new).unwrap();

    f.scan();

    assert!(f.suggestions().is_empty(), "both the former and the new path are covered");
    let workspaces = f.store.lock().unwrap().list_workspaces().unwrap();
    assert_eq!(attribution::owner(&old, &workspaces).map(|w| w.id), Some(id));
    assert_eq!(attribution::owner(&new, &workspaces).map(|w| w.id), Some(id));
}

#[test]
fn former_paths_take_part_in_nesting() {
    let f = Fixture::new();
    let outer = f.dir("code");
    let outer_id = f.add_workspace(&outer);
    let inner_id = f.add_workspace(&f.dir("code/app"));
    f.store.lock().unwrap().repoint_workspace(inner_id, &f.dir("moved")).unwrap();
    let workspaces = f.store.lock().unwrap().list_workspaces().unwrap();

    let owner = |p: &NormalizedPath| attribution::owner(p, &workspaces).map(|w| w.id);
    assert_eq!(owner(&f.dir("code/app/src")), Some(inner_id), "the deeper former path wins");
    assert_eq!(owner(&f.dir("code/lib")), Some(outer_id));
}

#[test]
fn removing_a_former_path_makes_its_sessions_a_suggestion_again() {
    let f = Fixture::new();
    let old = f.dir("old");
    f.transcript("p", "s1", old.as_str(), &["2026-10-24T04:00:00Z"]);
    let id = f.add_workspace(&old);
    f.store.lock().unwrap().repoint_workspace(id, &f.dir("new")).unwrap();
    f.scan();

    f.store.lock().unwrap().remove_former_path(id, &old).unwrap();

    assert_eq!(paths(&f.suggestions()), vec![old.to_string()]);
}
