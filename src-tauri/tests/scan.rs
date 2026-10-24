//! Scanning transcripts into durable Session records, and the Suggestions
//! computed from them on read (spec §4.3, §5.1, §5.4, §5.5, §6.6). Runs
//! against fixture transcript roots, never the user's real `~/.claude`.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use code_orchestrate_lib::agents::claude_code::ClaudeCode;
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
