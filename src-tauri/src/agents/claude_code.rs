//! The Claude Code adapter (spec §3.2, §5.2, §5.3, §7.1).

mod transcript;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{AgentAdapter, Capabilities, LaunchCommand, LaunchCommandError, LaunchMode, SessionSummary, SourceKey};
use crate::launch::{escape_for_cmd, reaches_cmd_quoted};
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
    /// Overrides `claude` on the search path: a file path, or a bare name
    /// looked up on the search path.
    executable: Option<PathBuf>,
}

/// The executable's status line (spec §9.3).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ExecutableStatus {
    /// Resolved; `version` is the first line `--version` printed, if any.
    Found { path: String, version: Option<String> },
    NotFound,
}

/// The transcript root's status line (spec §9.3).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum TranscriptRootStatus {
    Ok { path: String },
    /// The folder exists but has no `projects` folder.
    NoProjects { path: String },
    /// The folder does not exist (or no home directory is known: empty path).
    NotFound { path: String },
}

/// How long the `--version` probe may run before it is abandoned.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

impl ClaudeCode {
    /// Resolve `claude` on the process `PATH`.
    pub fn on_path() -> Self {
        ClaudeCode { search: SearchPath::Environment, transcript_root: None, executable: None }
    }

    /// Resolve `claude` in these directories, in order, instead of `PATH`.
    pub fn with_search_path(dirs: Vec<PathBuf>) -> Self {
        ClaudeCode { search: SearchPath::Dirs(dirs), transcript_root: None, executable: None }
    }

    /// Read transcripts from `root/projects/` instead of the default root.
    pub fn with_transcript_root(mut self, root: PathBuf) -> Self {
        self.transcript_root = Some(root);
        self
    }

    /// Run this executable instead of `claude` on the search path. A bare
    /// name (no folder) is looked up on the search path.
    pub fn with_executable(mut self, executable: PathBuf) -> Self {
        self.executable = Some(executable);
        self
    }

    /// The executable override, as set (`None`: `claude` on the search path).
    pub fn executable_override(&self) -> Option<String> {
        self.executable.as_ref().map(|p| p.to_string_lossy().into_owned())
    }

    /// Resolve the executable and probe it with `--version` (spec §9.3).
    pub fn executable_status(&self) -> ExecutableStatus {
        match self.resolve_executable() {
            None => ExecutableStatus::NotFound,
            Some(path) => {
                let version = probe_version(&path);
                ExecutableStatus::Found { path, version }
            }
        }
    }

    /// Whether the transcript root exists and holds a `projects` folder.
    pub fn transcript_root_status(&self) -> TranscriptRootStatus {
        let Some(root) = self.transcript_root() else {
            return TranscriptRootStatus::NotFound { path: String::new() };
        };
        let path = root.to_string_lossy().into_owned();
        if !root.is_dir() {
            TranscriptRootStatus::NotFound { path }
        } else if !root.join("projects").is_dir() {
            TranscriptRootStatus::NoProjects { path }
        } else {
            TranscriptRootStatus::Ok { path }
        }
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
        let found = match &self.executable {
            None => find_executable(EXECUTABLE_NAME, &self.search_dirs()),
            // Only an absolute path: a relative one would mean something
            // else in each Workspace directory the command starts in.
            Some(exe) if exe.is_absolute() => exe.is_file().then(|| exe.clone()),
            Some(exe) if exe.components().count() > 1 => None,
            Some(name) => {
                let dirs = self.search_dirs();
                // A name typed with its extension (`claude.exe`) is tried as is first.
                let as_typed = name.extension().is_some().then(|| dirs.iter().map(|d| d.join(name)).find(|p| is_file(p)));
                as_typed.flatten().or_else(|| find_executable(&name.to_string_lossy(), &dirs))
            }
        };
        found.map(|p| p.to_string_lossy().into_owned())
    }

    fn launch_command(
        &self,
        workspace_path: &NormalizedPath,
        mode: &LaunchMode,
    ) -> Result<LaunchCommand, LaunchCommandError> {
        let agent_args = match mode {
            LaunchMode::New => vec![],
            LaunchMode::Continue => vec!["--continue".to_owned()],
            LaunchMode::Resume(session_id) => vec!["--resume".to_owned(), session_id.clone()],
        };
        self.command(agent_args, workspace_path.as_str().to_owned())
    }

    fn version_command(&self, cwd: &str) -> Result<LaunchCommand, LaunchCommandError> {
        self.command(vec!["--version".to_owned()], cwd.to_owned())
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

impl ClaudeCode {
    /// Run the resolved executable with `agent_args` in `cwd`.
    fn command(&self, agent_args: Vec<String>, cwd: String) -> Result<LaunchCommand, LaunchCommandError> {
        let executable = self
            .resolve_executable()
            .ok_or_else(|| LaunchCommandError::ExecutableNotFound { name: self.executable_name() })?;
        Ok(if is_batch_script(&executable) {
            // A `.cmd` (npm shim) cannot be the direct target of process
            // creation; `cmd /k` runs it and keeps the tab open afterwards.
            let mut args = vec!["/k".to_owned()];
            // A quoted path right after `/k` loses its quotes when it holds a
            // special character such as `&`, so `@` goes first. Not `call`,
            // which doubles carets, even inside quotes.
            if reaches_cmd_quoted(&executable) {
                args.push("@".to_owned());
            }
            args.push(escape_for_cmd(&executable));
            args.extend(agent_args);
            LaunchCommand { program: "cmd".to_owned(), args, cwd }
        } else {
            LaunchCommand { program: executable, args: agent_args, cwd }
        })
    }
}

impl ClaudeCode {
    /// The name shown when the executable cannot be found.
    fn executable_name(&self) -> String {
        match &self.executable {
            Some(exe) => exe.to_string_lossy().into_owned(),
            None => EXECUTABLE_NAME.into(),
        }
    }
}

/// Run `executable --version` without a console window and return the first
/// non-empty line it printed. `None` when it fails, prints nothing or hangs.
fn probe_version(executable: &str) -> Option<String> {
    // Rust runs a `.cmd`/`.bat` through `cmd.exe` with safe argument quoting.
    let mut command = Command::new(executable);
    command.arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_owned)
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
