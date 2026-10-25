//! Workspace stats and Session lists computed on read from scanned Sessions
//! (spec §5.2, §5.4, §5.6). Runs against fixture transcript roots, never the
//! user's real `~/.claude`.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use code_orchestrate_lib::agents::TokenUsage;
use code_orchestrate_lib::agents::claude_code::ClaudeCode;
use code_orchestrate_lib::attribution::{self, WorkspaceStats};
use code_orchestrate_lib::paths::{NormalizedPath, normalize};
use code_orchestrate_lib::scan;
use code_orchestrate_lib::store::{NewWorkspace, Store};

struct Fixture {
    root: tempfile::TempDir,
    dirs: tempfile::TempDir,
    store: Mutex<Store>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("projects")).unwrap();
        Fixture { root, dirs: tempfile::tempdir().unwrap(), store: Mutex::new(Store::open_in_memory().unwrap()) }
    }

    fn dir(&self, name: &str) -> NormalizedPath {
        let path = self.dirs.path().join(name);
        fs::create_dir_all(&path).unwrap();
        normalize(path.to_str().unwrap()).unwrap()
    }

    /// Write `projects/<rel>` with these JSON lines.
    fn write(&self, rel: &str, lines: &[serde_json::Value]) -> PathBuf {
        let path = self.root.path().join("projects").join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        fs::write(&path, body).unwrap();
        path
    }

    /// A Session in `cwd` from `start` for `minutes`, with one response of `input` tokens.
    fn session(&self, id: &str, cwd: &NormalizedPath, start: &str, end: &str, input: i64) -> PathBuf {
        self.write(
            &format!("p/{id}.jsonl"),
            &[
                serde_json::json!({"type": "user", "cwd": cwd.as_str(), "timestamp": start,
                    "message": {"content": format!("prompt {id}")}}),
                assistant(&format!("{id}-msg"), input, end),
            ],
        )
    }

    fn scan(&self) -> scan::ScanReport {
        scan::run(&self.store, &ClaudeCode::on_path().with_transcript_root(self.root.path().to_path_buf())).unwrap()
    }

    fn add_workspace(&self, name: &str, path: &NormalizedPath) -> i64 {
        let store = self.store.lock().unwrap();
        store.add_workspace(NewWorkspace { name: name.into(), path: path.clone() }).unwrap().id
    }

    fn stats(&self, id: i64) -> WorkspaceStats {
        let all = attribution::workspace_stats(&self.store.lock().unwrap()).unwrap();
        all.into_iter().find(|s| s.workspace_id == id).expect("every Workspace has stats")
    }
}

fn assistant(id: &str, input: i64, timestamp: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "assistant", "timestamp": timestamp,
        "message": {"id": id, "usage": {
            "input_tokens": input, "output_tokens": 2,
            "cache_creation_input_tokens": 3, "cache_read_input_tokens": 1000,
        }}
    })
}

#[test]
fn a_workspace_totals_its_sessions() {
    let f = Fixture::new();
    let app = f.dir("app");
    let sub = f.dir("app/src");
    let id = f.add_workspace("app", &app);
    f.session("s1", &app, "2026-10-24T04:00:00Z", "2026-10-24T04:20:00Z", 10);
    f.session("s2", &sub, "2026-10-25T04:00:00Z", "2026-10-25T05:00:00Z", 100);
    f.scan();

    let stats = f.stats(id);

    assert_eq!(stats.session_count, 2);
    assert_eq!(stats.last_used, Some(1_792_904_400_000));
    assert_eq!(stats.session_time_ms, Some(20 * 60_000));
    assert_eq!(
        stats.tokens,
        TokenUsage { input: Some(110), output: Some(4), cache_write: Some(6), cache_read: Some(2000) }
    );
}

#[test]
fn a_session_in_a_nested_workspace_counts_toward_the_inner_one_only() {
    let f = Fixture::new();
    let outer = f.dir("a");
    let inner = f.dir("a/b");
    let outer_id = f.add_workspace("a", &outer);
    let inner_id = f.add_workspace("b", &inner);
    f.session("in-b", &f.dir("a/b/src"), "2026-10-24T04:00:00Z", "2026-10-24T04:01:00Z", 1);
    f.session("in-a", &outer, "2026-10-24T04:00:00Z", "2026-10-24T04:01:00Z", 1);
    f.session("in-a-too", &f.dir("a/c"), "2026-10-24T04:00:00Z", "2026-10-24T04:01:00Z", 1);
    f.scan();

    assert_eq!(f.stats(inner_id).session_count, 1);
    assert_eq!(f.stats(outer_id).session_count, 2);
}

#[test]
fn a_workspace_without_sessions_has_empty_stats() {
    let f = Fixture::new();
    let id = f.add_workspace("idle", &f.dir("idle"));

    let stats = f.stats(id);

    assert_eq!(stats.session_count, 0);
    assert_eq!(stats.last_used, None);
    assert_eq!(stats.session_time_ms, None);
    assert_eq!(stats.tokens, TokenUsage::default());
}

#[test]
fn a_workspaces_sessions_are_listed_newest_activity_first_with_their_stats() {
    let f = Fixture::new();
    let app = f.dir("app");
    let id = f.add_workspace("app", &app);
    f.session("old", &app, "2026-10-20T04:00:00Z", "2026-10-20T04:10:00Z", 5);
    f.session("new", &app, "2026-10-24T04:00:00Z", "2026-10-24T04:02:00Z", 7);
    f.session("elsewhere", &f.dir("other"), "2026-10-25T04:00:00Z", "2026-10-25T04:02:00Z", 7);
    f.scan();

    let sessions = attribution::workspace_sessions(&f.store.lock().unwrap(), id).unwrap();

    let ids: Vec<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
    assert_eq!(ids, vec!["new", "old"]);
    let old = &sessions[1];
    assert_eq!(old.title.as_deref(), Some("prompt old"));
    assert_eq!(old.active_time_ms, Some(10 * 60_000));
    assert_eq!(old.tokens.input, Some(5));
    assert_eq!(old.first_cwd, app);
}

#[test]
fn a_change_to_a_subagent_file_updates_its_parent_session() {
    let f = Fixture::new();
    let app = f.dir("app");
    let id = f.add_workspace("app", &app);
    f.session("s", &app, "2026-10-24T04:00:00Z", "2026-10-24T04:01:00Z", 10);
    f.scan();

    f.write("p/s/subagents/agent-x.jsonl", &[assistant("sub-msg", 1000, "2026-10-24T04:00:30Z")]);
    let report = f.scan();

    assert_eq!(report.parsed, 1);
    assert_eq!(f.stats(id).tokens.input, Some(1010));
    assert_eq!(f.stats(id).session_count, 1, "a subagent is not a Session");
}
