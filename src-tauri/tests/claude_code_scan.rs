//! The Claude Code adapter's Session scan (spec §3.2, §5.2), run against
//! fixture transcript directories, never the user's real `~/.claude`.

use std::fs;
use std::path::{Path, PathBuf};

use code_orchestrate_lib::agents::AgentAdapter;
use code_orchestrate_lib::agents::claude_code::ClaudeCode;

/// A fake `~/.claude` with a `projects/` folder.
struct Root {
    dir: tempfile::TempDir,
}

impl Root {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("projects")).unwrap();
        Root { dir }
    }

    fn agent(&self) -> ClaudeCode {
        ClaudeCode::on_path().with_transcript_root(self.dir.path().to_path_buf())
    }

    /// Write `projects/<folder>/<file>` with these lines, each newline-terminated.
    fn write(&self, folder: &str, file: &str, lines: &[&str]) -> PathBuf {
        let dir = self.dir.path().join("projects").join(folder);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(file);
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        fs::write(&path, body).unwrap();
        path
    }
}

const CWD: &str = if cfg!(windows) { r"E:\Programming\a-b" } else { "/programming/a-b" };
const OTHER_CWD: &str = if cfg!(windows) { r"E:\Programming\a_b" } else { "/programming/a_b" };

/// A `user` record with this cwd and timestamp.
fn user(cwd: &str, timestamp: &str) -> String {
    serde_json::json!({
        "type": "user", "cwd": cwd, "timestamp": timestamp, "isSidechain": false,
        "sessionId": "ignored", "message": {"role": "user", "content": "hi"}
    })
    .to_string()
}

fn parse(agent: &ClaudeCode, path: &Path) -> code_orchestrate_lib::agents::SessionSummary {
    agent.parse_source(path).expect("a Session")
}

#[test]
fn a_transcript_is_one_session_named_by_its_file() {
    let root = Root::new();
    let path = root.write(
        "E--Programming-a-b",
        "11111111-aaaa.jsonl",
        &[&user(CWD, "2026-10-24T04:53:06.933Z"), &user(CWD, "2026-10-24T05:00:00.000Z")],
    );
    let agent = root.agent();

    let session = parse(&agent, &path);

    assert_eq!(session.session_id, "11111111-aaaa");
    assert_eq!(session.first_cwd.as_str(), CWD);
    assert_eq!(session.started_at, Some(1_792_817_586_933));
    assert_eq!(session.last_activity_at, Some(1_792_818_000_000));
    assert_eq!(session.source_path, path);
}

#[test]
fn the_first_cwd_wins_and_later_changes_are_ignored() {
    let root = Root::new();
    let sub = format!("{CWD}{}sub", std::path::MAIN_SEPARATOR);
    let path = root.write(
        "E--Programming-a-b",
        "s.jsonl",
        &[
            r#"{"type":"ai-title","aiTitle":"No cwd here"}"#,
            &user(CWD, "2026-10-24T04:00:00Z"),
            &user(&sub, "2026-10-24T04:10:00Z"),
        ],
    );

    assert_eq!(parse(&root.agent(), &path).first_cwd.as_str(), CWD);
}

#[test]
fn unknown_records_bad_lines_and_a_half_written_last_line_are_skipped() {
    let root = Root::new();
    let dir = root.dir.path().join("projects").join("p");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    let body = format!(
        "{}\n{}\nnot json at all\n{}\n{}\n{{\"type\":\"user\",\"cwd\":\"X:\\\\half",
        r#"{"type":"some-future-record","weird":[1,2,3],"timestamp":"garbage"}"#,
        &user(CWD, "2026-10-24T04:00:00.5Z"),
        r#"{"type":"assistant"}"#,
        &user(CWD, "2026-10-24T06:00:00+02:00"),
    );
    fs::write(&path, body).unwrap();

    let session = parse(&root.agent(), &path);
    assert_eq!(session.first_cwd.as_str(), CWD);
    // 06:00+02:00 is 04:00:00Z; the other record is 04:00:00.5Z.
    assert_eq!(session.started_at, Some(1_792_814_400_000));
    assert_eq!(session.last_activity_at, Some(1_792_814_400_500));
}

#[test]
fn sidechain_records_do_not_count() {
    let root = Root::new();
    let sidechain = serde_json::json!({
        "type": "user", "cwd": OTHER_CWD, "timestamp": "2026-10-20T00:00:00Z", "isSidechain": true
    })
    .to_string();
    let path = root.write("p", "s.jsonl", &[&sidechain, &user(CWD, "2026-10-24T04:00:00Z")]);

    let session = parse(&root.agent(), &path);
    assert_eq!(session.first_cwd.as_str(), CWD);
    assert_eq!(session.started_at, Some(1_792_814_400_000));
}

#[test]
fn a_transcript_with_no_working_directory_is_not_a_session_yet() {
    let root = Root::new();
    let path = root.write("p", "s.jsonl", &[r#"{"type":"ai-title","aiTitle":"t"}"#]);
    assert_eq!(root.agent().parse_source(&path), None);
}

#[test]
fn sources_are_the_top_level_transcripts_with_their_size() {
    let root = Root::new();
    let a = root.write("E--Programming-a-b", "one.jsonl", &[&user(CWD, "2026-10-24T04:00:00Z")]);
    let b = root.write("E--Programming-a-b", "two.jsonl", &[&user(OTHER_CWD, "2026-10-24T04:00:00Z")]);
    root.write("E--Programming-a-b/one/subagents", "agent-x.jsonl", &[&user(CWD, "2026-10-24T04:00:00Z")]);
    root.write("E--Programming-a-b/memory", "MEMORY.md", &["notes"]);
    root.write("D--memory-only/memory", "MEMORY.md", &["notes"]);

    let sources = root.agent().list_sources();

    let paths: Vec<_> = sources.iter().map(|s| s.path.clone()).collect();
    assert_eq!(paths, vec![a.clone(), b]);
    assert_eq!(sources[0].size, fs::metadata(&a).unwrap().len());
    assert!(sources[0].mtime > 0);
}

#[test]
fn a_missing_transcript_root_has_no_sources() {
    let dir = tempfile::tempdir().unwrap();
    let agent = ClaudeCode::on_path().with_transcript_root(dir.path().join("nope"));
    assert!(agent.list_sources().is_empty());
}
