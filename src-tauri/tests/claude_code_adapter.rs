//! The Claude Code Agent adapter: identity and launch command (spec §3.2, §7.1).

use std::path::Path;

use code_orchestrate_lib::agents::claude_code::ClaudeCode;
use code_orchestrate_lib::agents::{
    AgentAdapter, Capabilities, LaunchCommandError, LaunchMode, agent_status,
};
use code_orchestrate_lib::paths::{NormalizedPath, normalize};

fn touch(dir: &Path, file: &str) -> String {
    let path = dir.join(file);
    std::fs::write(&path, "").unwrap();
    path.to_str().unwrap().to_owned()
}

fn workspace() -> NormalizedPath {
    normalize(r"C:\code\my project").unwrap()
}

#[test]
fn identifies_as_claude_code_with_every_capability() {
    let agent = ClaudeCode::on_path();
    assert_eq!(agent.id(), "claude-code");
    assert_eq!(agent.display_name(), "Claude Code");
    assert_eq!(
        agent.capabilities(),
        Capabilities { continue_last: true, resume: true, session_history: true }
    );
}

#[test]
fn new_session_runs_the_executable_found_on_the_search_path_in_the_workspace() {
    let bin = tempfile::tempdir().unwrap();
    let exe = touch(bin.path(), "claude.exe");
    let agent = ClaudeCode::with_search_path(vec![bin.path().to_path_buf()]);

    let cmd = agent.launch_command(&workspace(), &LaunchMode::New).unwrap();

    assert_eq!(cmd.program, exe);
    assert!(cmd.args.is_empty(), "{:?}", cmd.args);
    assert_eq!(cmd.cwd, workspace().as_str());
}

#[test]
fn continue_last_adds_the_continue_flag() {
    let bin = tempfile::tempdir().unwrap();
    touch(bin.path(), "claude.exe");
    let agent = ClaudeCode::with_search_path(vec![bin.path().to_path_buf()]);

    let cmd = agent.launch_command(&workspace(), &LaunchMode::Continue).unwrap();

    assert_eq!(cmd.args, ["--continue"]);
}

#[test]
fn resume_passes_the_session_id() {
    let bin = tempfile::tempdir().unwrap();
    touch(bin.path(), "claude.exe");
    let agent = ClaudeCode::with_search_path(vec![bin.path().to_path_buf()]);

    let mode = LaunchMode::Resume("0b9f-session".into());
    let cmd = agent.launch_command(&workspace(), &mode).unwrap();

    assert_eq!(cmd.args, ["--resume", "0b9f-session"]);
}

#[test]
fn the_version_command_runs_claude_version_in_the_given_directory() {
    let bin = tempfile::tempdir().unwrap();
    let exe = touch(bin.path(), if cfg!(windows) { "claude.exe" } else { "claude" });
    let agent = ClaudeCode::with_search_path(vec![bin.path().to_path_buf()]);

    let cmd = agent.version_command(r"C:\Users\me").unwrap();

    assert_eq!(cmd.program, exe);
    assert_eq!(cmd.args, ["--version"]);
    assert_eq!(cmd.cwd, r"C:\Users\me");
}

#[cfg(windows)]
#[test]
fn the_version_command_wraps_an_npm_shim_too() {
    let bin = tempfile::tempdir().unwrap();
    let shim = touch(bin.path(), "claude.cmd");
    let agent = ClaudeCode::with_search_path(vec![bin.path().to_path_buf()]);

    let cmd = agent.version_command(r"C:\Users\me").unwrap();

    assert_eq!(cmd.program, "cmd");
    assert_eq!(cmd.args, ["/k".to_owned(), shim, "--version".to_owned()]);
}

#[cfg(windows)]
#[test]
fn an_npm_cmd_shim_is_wrapped_in_cmd_k() {
    let bin = tempfile::tempdir().unwrap();
    let shim = touch(bin.path(), "claude.cmd");
    let agent = ClaudeCode::with_search_path(vec![bin.path().to_path_buf()]);

    let cmd = agent.launch_command(&workspace(), &LaunchMode::Continue).unwrap();

    assert_eq!(cmd.program, "cmd");
    assert_eq!(cmd.args, ["/k".to_owned(), shim, "--continue".to_owned()]);
    assert_eq!(cmd.cwd, workspace().as_str());
}

#[cfg(windows)]
#[test]
fn cmd_metacharacters_in_an_unquoted_shim_path_are_caret_escaped() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("Tom&Jerry");
    std::fs::create_dir(&bin).unwrap();
    let shim = touch(&bin, "claude.cmd");
    let agent = ClaudeCode::with_search_path(vec![bin.clone()]);

    let cmd = agent.launch_command(&workspace(), &LaunchMode::New).unwrap();

    assert_eq!(cmd.args, ["/k".to_owned(), shim.replace('&', "^&")]);
}

/// cmd strips the outer quotes of a command line that starts with a quote and
/// holds a special character, splitting `Tools & Bin` at the `&`. `@` keeps
/// the quoted path away from the start of the line. (Checked by hand against
/// real cmd.exe and wt.exe, with `&` and with `^`: a stub `.cmd` runs. A
/// generated npm shim still fails there, on its own unquoted `SET dp0=%~dp0`.)
#[cfg(windows)]
#[test]
fn a_shim_path_with_whitespace_follows_an_at_sign_so_cmd_keeps_its_quotes() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("Tools & Bin");
    std::fs::create_dir(&bin).unwrap();
    let shim = touch(&bin, "claude.cmd");
    let agent = ClaudeCode::with_search_path(vec![bin]);

    let cmd = agent.launch_command(&workspace(), &LaunchMode::Continue).unwrap();

    assert_eq!(cmd.program, "cmd");
    assert_eq!(cmd.args, ["/k".to_owned(), "@".to_owned(), shim, "--continue".to_owned()]);
}

/// The Console window preset runs the shim through two cmd parses (its own
/// `cmd /c start`, then the adapter's `cmd /k`), each stripping one level of
/// carets, so `^` reaches it as `^^^^` and `&` as `^^^&`. (Checked by hand
/// against real cmd.exe: the shim runs.)
#[cfg(windows)]
#[test]
fn a_console_window_escapes_an_npm_shim_path_once_per_cmd_layer() {
    use code_orchestrate_lib::launch::{Terminal, TerminalCommand};

    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_str().unwrap().to_owned();
    assert!(!root_path.contains(|c: char| c.is_whitespace() || "&|<>^".contains(c)), "{root_path}");
    let bin = root.path().join("a^b&c");
    std::fs::create_dir(&bin).unwrap();
    touch(&bin, "claude.cmd");
    let agent = ClaudeCode::with_search_path(vec![bin]);
    let cmd = agent.launch_command(&workspace(), &LaunchMode::Resume("s1".into())).unwrap();

    let wrapped = Terminal::ConsoleWindow.wrap(&cmd);

    let shim = format!(r"{root_path}\a^^^^b^^^&c\claude.cmd");
    let args = ["/c", "start", "", "/D", r"C:\code\my project", "cmd", "/k", &shim, "--resume", "s1"];
    assert_eq!(
        wrapped,
        TerminalCommand { program: "cmd.exe".into(), args: args.map(str::to_owned).to_vec(), current_dir: None }
    );
}

#[cfg(windows)]
#[test]
fn the_first_directory_on_the_search_path_wins() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let shim = touch(first.path(), "claude.cmd");
    touch(second.path(), "claude.exe");
    let agent =
        ClaudeCode::with_search_path(vec![first.path().to_path_buf(), second.path().to_path_buf()]);

    assert_eq!(agent.resolve_executable(), Some(shim));
}

#[cfg(windows)]
#[test]
fn an_exe_beats_a_cmd_shim_in_the_same_directory() {
    let bin = tempfile::tempdir().unwrap();
    let exe = touch(bin.path(), "claude.exe");
    touch(bin.path(), "claude.cmd");
    let agent = ClaudeCode::with_search_path(vec![bin.path().to_path_buf()]);

    assert_eq!(agent.resolve_executable(), Some(exe));
}

#[test]
fn status_tells_the_ui_who_the_agent_is_and_where_its_executable_is() {
    let bin = tempfile::tempdir().unwrap();
    let exe = touch(bin.path(), if cfg!(windows) { "claude.exe" } else { "claude" });
    let found = ClaudeCode::with_search_path(vec![bin.path().to_path_buf()]);
    let empty = tempfile::tempdir().unwrap();
    let missing = ClaudeCode::with_search_path(vec![empty.path().to_path_buf()]);

    assert_eq!(
        serde_json::to_value(agent_status(&found)).unwrap(),
        serde_json::json!({
            "id": "claude-code",
            "displayName": "Claude Code",
            "capabilities": { "continue": true, "resume": true, "sessionHistory": true },
            "executable": exe,
        })
    );
    assert_eq!(serde_json::to_value(agent_status(&missing)).unwrap()["executable"], serde_json::Value::Null);
}

#[test]
fn reports_the_executable_missing_when_it_is_not_on_the_search_path() {
    let empty = tempfile::tempdir().unwrap();
    let agent = ClaudeCode::with_search_path(vec![empty.path().to_path_buf()]);

    assert_eq!(agent.resolve_executable(), None);
    let err = agent.launch_command(&workspace(), &LaunchMode::New).unwrap_err();
    assert_eq!(err, LaunchCommandError::ExecutableNotFound { name: "claude".into() });
}
