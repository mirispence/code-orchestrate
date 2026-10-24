//! The Claude Code adapter (spec §3.2, §5.2, §5.3, §7.1).

mod transcript;

use std::path::{Path, PathBuf};

use super::{AgentAdapter, Capabilities, LaunchCommand, LaunchCommandError, LaunchMode, SessionSummary, SourceKey};
use crate::paths::NormalizedPath;

/// The executable's bare name, looked up on the search path.
pub const EXECUTABLE_NAME: &str = "claude";

/// Where to look for `claude`.
#[derive(Debug, Clone)]
enum SearchPath {
    /// The process's `PATH`, read on every lookup.
    Environment,
    /// Fixed directories (tests).
    Dirs(Vec<PathBuf>),
}

#[derive(Debug, Clone)]
pub struct ClaudeCode {
    search: SearchPath,
    /// Overrides the default transcript root (the folder holding `projects/`).
    transcript_root: Option<PathBuf>,
}

impl ClaudeCode {
    /// Resolve `claude` on the process `PATH`.
    pub fn on_path() -> Self {
        ClaudeCode { search: SearchPath::Environment, transcript_root: None }
    }

    /// Resolve `claude` in these directories, in order, instead of `PATH`.
    pub fn with_search_path(dirs: Vec<PathBuf>) -> Self {
        ClaudeCode { search: SearchPath::Dirs(dirs), transcript_root: None }
    }

    /// Read transcripts from `root/projects/` instead of the default root.
    pub fn with_transcript_root(mut self, root: PathBuf) -> Self {
        self.transcript_root = Some(root);
        self
    }

    /// The transcript root (spec §5.3): the override, else `CLAUDE_CONFIG_DIR`,
    /// else `~/.claude`. `None` when no home directory is known.
    pub fn transcript_root(&self) -> Option<PathBuf> {
        if let Some(root) = &self.transcript_root {
            return Some(root.clone());
        }
        if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
            return Some(PathBuf::from(dir));
        }
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .filter(|d| !d.is_empty())?;
        Some(PathBuf::from(home).join(".claude"))
    }

    fn search_dirs(&self) -> Vec<PathBuf> {
        match &self.search {
            SearchPath::Environment => std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            SearchPath::Dirs(dirs) => dirs.clone(),
        }
    }
}

impl AgentAdapter for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { continue_last: true, resume: true, session_history: true }
    }

    fn resolve_executable(&self) -> Option<String> {
        find_executable(EXECUTABLE_NAME, &self.search_dirs())
            .map(|p| p.to_string_lossy().into_owned())
    }

    fn launch_command(
        &self,
        workspace_path: &NormalizedPath,
        mode: &LaunchMode,
    ) -> Result<LaunchCommand, LaunchCommandError> {
        let executable = self
            .resolve_executable()
            .ok_or_else(|| LaunchCommandError::ExecutableNotFound { name: EXECUTABLE_NAME.into() })?;
        let agent_args = match mode {
            LaunchMode::New => vec![],
            LaunchMode::Continue => vec!["--continue".to_owned()],
            LaunchMode::Resume(session_id) => vec!["--resume".to_owned(), session_id.clone()],
        };
        let cwd = workspace_path.as_str().to_owned();
        Ok(if is_batch_script(&executable) {
            // A `.cmd` (npm shim) cannot be the direct target of process
            // creation; `cmd /k` runs it and keeps the tab open afterwards.
            let mut args = vec!["/k".to_owned(), escape_for_cmd(&executable)];
            args.extend(agent_args);
            LaunchCommand { program: "cmd".to_owned(), args, cwd }
        } else {
            LaunchCommand { program: executable, args: agent_args, cwd }
        })
    }

    fn list_sources(&self) -> Vec<SourceKey> {
        match self.transcript_root() {
            Some(root) => transcript::list_sources(&root.join("projects")),
            None => Vec::new(),
        }
    }

    fn parse_source(&self, path: &Path) -> std::io::Result<Option<SessionSummary>> {
        transcript::parse(path)
    }
}

/// `cmd /k` reparses its command line, so an unquoted `&`, `|`, `<`, `>` or
/// `^` (legal in a Windows user name, hence in an npm prefix) would split or
/// redirect it. A path without whitespace reaches cmd unquoted, so caret-escape
/// those; a path with whitespace is quoted, and inside quotes they are literal.
fn escape_for_cmd(arg: &str) -> String {
    if arg.chars().any(char::is_whitespace) {
        return arg.to_owned();
    }
    let mut escaped = String::with_capacity(arg.len());
    for c in arg.chars() {
        if matches!(c, '&' | '|' | '<' | '>' | '^') {
            escaped.push('^');
        }
        escaped.push(c);
    }
    escaped
}

/// A `.cmd` or `.bat` file, which only `cmd` can run.
fn is_batch_script(executable: &str) -> bool {
    Path::new(executable)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
}

/// The first `name` + extension that is a file, directory by directory.
fn find_executable(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let extensions = executable_extensions();
    dirs.iter().find_map(|dir| {
        extensions.iter().map(|ext| dir.join(format!("{name}{ext}"))).find(|p| is_file(p))
    })
}

fn executable_extensions() -> Vec<String> {
    // A native install is `claude.exe`; the npm install is a `claude.cmd` shim.
    if cfg!(windows) { vec![".exe".into(), ".cmd".into()] } else { vec![String::new()] }
}

fn is_file(path: &Path) -> bool {
    path.is_file()
}
