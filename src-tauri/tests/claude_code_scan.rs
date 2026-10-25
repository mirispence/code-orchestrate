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
    agent.parse_source(path).expect("readable").expect("a Session")
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
fn malformed_non_ascii_timestamps_are_skipped() {
    let root = Root::new();
    let odd = serde_json::json!({"type": "user", "cwd": CWD, "timestamp": "2026-10-24T04:00:00éabcd"}).to_string();
    let path = root.write("p", "s.jsonl", &[&odd, &user(CWD, "2026-10-24T04:00:00Z")]);

    let session = parse(&root.agent(), &path);
    assert_eq!(session.started_at, Some(1_792_814_400_000));
    assert_eq!(session.last_activity_at, Some(1_792_814_400_000));
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
    assert_eq!(root.agent().parse_source(&path).unwrap(), None);
}

#[test]
fn sources_are_the_top_level_transcripts_with_their_size() {
    let root = Root::new();
    let a = root.write("E--Programming-a-b", "one.jsonl", &[&user(CWD, "2026-10-24T04:00:00Z")]);
    let b = root.write("E--Programming-a-b", "two.jsonl", &[&user(OTHER_CWD, "2026-10-24T04:00:00Z")]);
    let sub = root.write("E--Programming-a-b/one/subagents", "agent-x.jsonl", &[&user(CWD, "2026-10-24T04:00:00Z")]);
    root.write("E--Programming-a-b/memory", "MEMORY.md", &["notes"]);
    root.write("D--memory-only/memory", "MEMORY.md", &["notes"]);

    let sources = root.agent().list_sources();

    let paths: Vec<_> = sources.iter().map(|s| s.path.clone()).collect();
    assert_eq!(paths, vec![a.clone(), b.clone()]);
    // A Session's size includes its subagent files.
    assert_eq!(sources[0].size, fs::metadata(&a).unwrap().len() + fs::metadata(&sub).unwrap().len());
    assert_eq!(sources[1].size, fs::metadata(&b).unwrap().len());
    assert!(sources[0].mtime > 0);
}

#[test]
fn a_missing_transcript_root_has_no_sources() {
    let dir = tempfile::tempdir().unwrap();
    let agent = ClaudeCode::on_path().with_transcript_root(dir.path().join("nope"));
    assert!(agent.list_sources().is_empty());
}

#[test]
fn active_time_sums_gaps_and_counts_a_gap_over_30_minutes_as_zero() {
    let root = Root::new();
    let path = root.write(
        "p",
        "s.jsonl",
        &[
            // Out of order on purpose: records are sorted by time first.
            &user(CWD, "2026-10-24T04:10:00Z"),
            &user(CWD, "2026-10-24T04:00:00Z"),
            r#"{"type":"ai-title","aiTitle":"untimed records are skipped"}"#,
            // 30 minutes exactly still counts.
            &user(CWD, "2026-10-24T04:40:00Z"),
            // 30 minutes and 1 second is idle: zero.
            &user(CWD, "2026-10-24T05:10:01Z"),
            &user(CWD, "2026-10-24T05:15:01Z"),
        ],
    );

    // 10m + 30m + 0 + 5m.
    assert_eq!(parse(&root.agent(), &path).active_time_ms, Some(45 * 60_000));
}

/// An `assistant` record for response `id` with this usage.
fn assistant(id: &str, input: i64, output: i64, cache_write: i64, cache_read: i64) -> String {
    serde_json::json!({
        "type": "assistant", "cwd": CWD, "timestamp": "2026-10-24T04:00:00Z",
        "message": {"id": id, "role": "assistant", "usage": {
            "input_tokens": input, "output_tokens": output,
            "cache_creation_input_tokens": cache_write, "cache_read_input_tokens": cache_read,
        }}
    })
    .to_string()
}

#[test]
fn tokens_count_each_response_once_by_message_id() {
    let root = Root::new();
    let path = root.write(
        "p",
        "s.jsonl",
        &[
            &user(CWD, "2026-10-24T04:00:00Z"),
            // One response written as three records repeating its usage.
            &assistant("msg_1", 10, 20, 300, 4000),
            &assistant("msg_1", 10, 20, 300, 4000),
            &assistant("msg_1", 10, 20, 300, 4000),
            &assistant("msg_2", 1, 2, 3, 4),
            // Usage on a user record is not counted.
            r#"{"type":"user","message":{"id":"msg_3","usage":{"input_tokens":999}}}"#,
        ],
    );

    let tokens = parse(&root.agent(), &path).tokens;

    assert_eq!(tokens.input, Some(11));
    assert_eq!(tokens.output, Some(22));
    assert_eq!(tokens.cache_write, Some(303));
    assert_eq!(tokens.cache_read, Some(4004));
}

/// A subagent record: sidechain, parent's session id, its own working directory.
fn subagent(id: &str, input: i64, timestamp: &str) -> String {
    serde_json::json!({
        "type": "assistant", "cwd": OTHER_CWD, "timestamp": timestamp, "isSidechain": true,
        "agentId": "x", "sessionId": "s",
        "message": {"id": id, "usage": {
            "input_tokens": input, "output_tokens": 1,
            "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0,
        }}
    })
    .to_string()
}

#[test]
fn subagent_tokens_are_added_to_the_parent_but_not_their_time_or_cwd() {
    let root = Root::new();
    let path = root.write(
        "p",
        "s.jsonl",
        &[&user(CWD, "2026-10-24T04:00:00Z"), &assistant("main", 100, 0, 0, 0), &user(CWD, "2026-10-24T04:05:00Z")],
    );
    root.write(
        "p/s/subagents",
        "agent-a.jsonl",
        &[&subagent("sub_1", 10, "2026-10-24T04:01:00Z"), &subagent("sub_1", 10, "2026-10-24T04:01:00Z")],
    );
    root.write("p/s/subagents", "agent-b.jsonl", &[&subagent("sub_2", 1, "2026-10-24T09:00:00Z")]);
    root.write("p/s/subagents", "agent-a.meta.json", &[r#"{"agentType":"x"}"#]);

    let session = parse(&root.agent(), &path);

    assert_eq!(session.tokens.input, Some(111));
    assert_eq!(session.tokens.output, Some(2));
    assert_eq!(session.first_cwd.as_str(), CWD);
    assert_eq!(session.last_activity_at, Some(1_792_814_700_000));
    assert_eq!(session.active_time_ms, Some(5 * 60_000));
}

#[test]
fn a_change_to_a_subagent_file_changes_its_parents_source_key() {
    let root = Root::new();
    let main = root.write("p", "s.jsonl", &[&user(CWD, "2026-10-24T04:00:00Z")]);
    let key = |agent: &ClaudeCode| {
        let sources = agent.list_sources();
        assert_eq!(sources.len(), 1, "subagent files are not Sessions");
        assert_eq!(sources[0].path, main);
        (sources[0].mtime, sources[0].size)
    };
    let agent = root.agent();
    let alone = key(&agent);

    root.write("p/s/subagents", "agent-a.jsonl", &[&subagent("sub_1", 10, "2026-10-24T04:01:00Z")]);
    let with_subagent = key(&agent);
    root.write(
        "p/s/subagents",
        "agent-a.jsonl",
        &[&subagent("sub_1", 10, "2026-10-24T04:01:00Z"), &subagent("sub_2", 10, "2026-10-24T04:02:00Z")],
    );
    let grown = key(&agent);

    assert_ne!(alone, with_subagent);
    assert_ne!(with_subagent, grown);
}

/// A user prompt record; `content` is the message content (string or blocks).
fn prompt(content: serde_json::Value) -> String {
    serde_json::json!({
        "type": "user", "cwd": CWD, "timestamp": "2026-10-24T04:00:00Z",
        "message": {"role": "user", "content": content}
    })
    .to_string()
}

fn title_of(lines: &[&str]) -> Option<String> {
    let root = Root::new();
    let path = root.write("p", "s.jsonl", lines);
    parse(&root.agent(), &path).title
}

#[test]
fn the_title_is_the_user_set_name_first() {
    let lines = [
        prompt("first prompt".into()),
        r#"{"type":"custom-title","customTitle":"Old name"}"#.to_owned(),
        r#"{"type":"ai-title","aiTitle":"AI title"}"#.to_owned(),
        r#"{"type":"last-prompt","lastPrompt":"last prompt"}"#.to_owned(),
        r#"{"type":"custom-title","customTitle":"My name"}"#.to_owned(),
    ];
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(title_of(&lines).as_deref(), Some("My name"));
}

#[test]
fn the_title_falls_back_to_the_latest_ai_title() {
    let lines = [
        prompt("first prompt".into()),
        r#"{"type":"ai-title","aiTitle":"Early title"}"#.to_owned(),
        r#"{"type":"last-prompt","lastPrompt":"last prompt"}"#.to_owned(),
        r#"{"type":"ai-title","aiTitle":"Later title"}"#.to_owned(),
    ];
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(title_of(&lines).as_deref(), Some("Later title"));
}

#[test]
fn the_title_falls_back_to_the_latest_last_prompt() {
    let lines = [
        prompt("first prompt".into()),
        r#"{"type":"last-prompt","lastPrompt":"earlier"}"#.to_owned(),
        r#"{"type":"last-prompt","lastPrompt":"the latest prompt"}"#.to_owned(),
    ];
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(title_of(&lines).as_deref(), Some("the latest prompt"));
}

#[test]
fn the_title_falls_back_to_the_first_user_prompt_truncated() {
    let long = format!("Fix the scanner{}", " and more".repeat(30));
    let lines = [
        // Meta records and tool results are not prompts.
        serde_json::json!({"type": "user", "cwd": CWD, "isMeta": true, "message": {"content": "caveat"}}).to_string(),
        prompt(serde_json::json!([{"type": "tool_result", "content": "output"}])),
        prompt(serde_json::json!([{"type": "text", "text": long}])),
        prompt("second prompt".into()),
    ];
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();

    let title = title_of(&lines).unwrap();
    assert!(title.starts_with("Fix the scanner and more"), "{title}");
    assert!(title.ends_with('…'));
    assert!(title.chars().count() <= 80, "{title}");
}

#[test]
fn a_session_with_no_prompt_has_no_title() {
    assert_eq!(title_of(&[&user(CWD, "2026-10-24T04:00:00Z").replace("\"hi\"", "[]")]), None);
}
