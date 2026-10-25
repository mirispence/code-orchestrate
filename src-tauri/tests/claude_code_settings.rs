//! Settings › Agents › Claude Code: the executable and transcript root
//! overrides and their status lines (spec §5.3, §9.1, §9.3). Uses temporary
//! folders only, never the user's real `~/.claude`.

use std::path::{Path, PathBuf};

use code_orchestrate_lib::agents::AgentAdapter;
use code_orchestrate_lib::agents::claude_code::{ClaudeCode, ExecutableStatus, TranscriptRootStatus};
use code_orchestrate_lib::settings::SettingsFile;
use serde_json::json;

fn settings(dir: &Path) -> SettingsFile {
    SettingsFile::new(dir.join("settings.json"))
}

fn touch(path: &Path) -> String {
    std::fs::write(path, "").unwrap();
    path.to_str().unwrap().to_owned()
}

#[test]
fn the_overrides_default_to_blank() {
    let dir = tempfile::tempdir().unwrap();
    let snap = settings(dir.path()).load();
    assert_eq!(snap.values["agents.claude-code.executable"], json!(""));
    assert_eq!(snap.values["agents.claude-code.transcriptRoot"], json!(""));
}

#[test]
fn an_override_must_be_text() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.json"), r#"{"agents":{"claude-code":{"executable":5}}}"#).unwrap();
    let snap = settings(dir.path()).load();
    assert_eq!(snap.values["agents.claude-code.executable"], json!(""));
    assert!(snap.warnings.contains_key("agents.claude-code.executable"));
}

#[test]
fn the_executable_override_is_launched_instead_of_claude_on_path() {
    let dir = tempfile::tempdir().unwrap();
    let exe = touch(&dir.path().join("my-claude.exe"));
    let file = settings(dir.path());
    let snap = file.set("agents.claude-code.executable", json!(exe)).unwrap();

    let agent = snap.claude_code();

    assert_eq!(agent.resolve_executable(), Some(exe.clone()));
    let cmd = agent.version_command(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(cmd.program, exe);
}

#[test]
fn a_missing_executable_override_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.exe");
    let snap = settings(dir.path()).set("agents.claude-code.executable", json!(missing.to_str().unwrap())).unwrap();

    let agent = snap.claude_code();

    assert_eq!(agent.resolve_executable(), None);
    assert_eq!(agent.executable_status(), ExecutableStatus::NotFound);
}

#[test]
fn the_transcript_root_override_is_scanned_instead_of_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("cc");
    let project = root.join("projects").join("p");
    std::fs::create_dir_all(&project).unwrap();
    touch(&project.join("s1.jsonl"));
    let snap = settings(dir.path()).set("agents.claude-code.transcriptRoot", json!(root.to_str().unwrap())).unwrap();

    let agent = snap.claude_code();

    assert_eq!(agent.transcript_root(), Some(root.clone()));
    let sources: Vec<PathBuf> = agent.list_sources().into_iter().map(|k| k.path).collect();
    assert_eq!(sources, vec![project.join("s1.jsonl")]);
}

#[test]
fn a_transcript_root_with_projects_is_ok() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("projects")).unwrap();
    let agent = ClaudeCode::on_path().with_transcript_root(dir.path().to_path_buf());
    assert_eq!(
        agent.transcript_root_status(),
        TranscriptRootStatus::Ok { path: dir.path().to_str().unwrap().to_owned() }
    );
}

#[test]
fn a_transcript_root_without_projects_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let agent = ClaudeCode::on_path().with_transcript_root(dir.path().to_path_buf());
    assert_eq!(
        agent.transcript_root_status(),
        TranscriptRootStatus::NoProjects { path: dir.path().to_str().unwrap().to_owned() }
    );
}

#[test]
fn a_missing_transcript_root_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("gone");
    let agent = ClaudeCode::on_path().with_transcript_root(root.clone());
    assert_eq!(
        agent.transcript_root_status(),
        TranscriptRootStatus::NotFound { path: root.to_str().unwrap().to_owned() }
    );
}

#[cfg(windows)]
#[test]
fn the_executable_is_probed_for_its_version() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("claude.cmd");
    std::fs::write(&exe, "@echo 2.1.0 (Claude Code)\r\n").unwrap();
    let agent = ClaudeCode::with_search_path(vec![dir.path().to_path_buf()]);

    assert_eq!(
        agent.executable_status(),
        ExecutableStatus::Found { path: exe.to_str().unwrap().to_owned(), version: Some("2.1.0 (Claude Code)".into()) }
    );
}
