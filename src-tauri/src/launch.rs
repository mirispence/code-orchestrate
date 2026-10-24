//! Terminal wrapping and spawning (spec §7.2, §7.3). The Agent adapter says
//! what to run; this module decides how it reaches a terminal window.

use std::io;

use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

use crate::agents::{AgentAdapter, LaunchCommand, LaunchCommandError, LaunchMode};
use crate::store::Store;

/// Why a launch failed. Serialises for the UI as `{ kind, message, ...fields }`.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum LaunchError {
    #[error("{name} was not found")]
    ExecutableNotFound { name: String },
    #[error("Workspace {workspace_id} does not exist")]
    WorkspaceNotFound { workspace_id: i64 },
    #[error("Could not start {program}: {os_error}")]
    SpawnFailed { program: String, os_error: String },
    /// The chosen terminal preset cannot be used as configured.
    #[error("{message}")]
    TerminalNotConfigured { message: String },
    #[error("{0}")]
    Internal(String),
}

impl From<LaunchCommandError> for LaunchError {
    fn from(e: LaunchCommandError) -> Self {
        match e {
            LaunchCommandError::ExecutableNotFound { name } => LaunchError::ExecutableNotFound { name },
        }
    }
}

impl Serialize for LaunchError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        let kind = match self {
            Self::ExecutableNotFound { .. } => "executableNotFound",
            Self::WorkspaceNotFound { .. } => "workspaceNotFound",
            Self::SpawnFailed { .. } => "spawnFailed",
            Self::TerminalNotConfigured { .. } => "terminalNotConfigured",
            Self::Internal(_) => "internal",
        };
        map.serialize_entry("kind", kind)?;
        map.serialize_entry("message", &self.to_string())?;
        match self {
            Self::ExecutableNotFound { name } => map.serialize_entry("name", name)?,
            Self::WorkspaceNotFound { workspace_id } => map.serialize_entry("workspaceId", workspace_id)?,
            Self::SpawnFailed { program, os_error } => {
                map.serialize_entry("program", program)?;
                map.serialize_entry("osError", os_error)?;
            }
            Self::TerminalNotConfigured { .. } | Self::Internal(_) => {}
        }
        map.end()
    }
}

/// Starts a process. The real one is [`OsSpawner`]; tests record instead.
pub trait Spawner {
    fn spawn(&self, cmd: &TerminalCommand) -> io::Result<()>;
}

/// Spawns with an argv array through `std::process::Command`: no shell, no
/// PowerShell. Rust quotes each argument (doubling backslashes before a quote,
/// which keeps a drive root like `C:\` intact). Nothing is monitored after.
pub struct OsSpawner;

impl Spawner for OsSpawner {
    fn spawn(&self, cmd: &TerminalCommand) -> io::Result<()> {
        let mut command = std::process::Command::new(&cmd.program);
        command.args(&cmd.args);
        if let Some(dir) = &cmd.current_dir {
            command.current_dir(dir);
        }
        command.spawn().map(drop)
    }
}

/// Start a Session for a Workspace: the adapter builds the command (resolving
/// its executable afresh), `terminal` wraps it, `spawner` starts it.
pub fn launch_session(
    store: &Store,
    agent: &dyn AgentAdapter,
    terminal: Terminal,
    spawner: &dyn Spawner,
    workspace_id: i64,
    mode: &LaunchMode,
) -> Result<(), LaunchError> {
    let workspaces = store.list_workspaces().map_err(|e| LaunchError::Internal(e.to_string()))?;
    let workspace = workspaces
        .into_iter()
        .find(|ws| ws.id == workspace_id)
        .ok_or(LaunchError::WorkspaceNotFound { workspace_id })?;
    let command = agent.launch_command(&workspace.path, mode)?;
    spawn_in(&terminal, spawner, &command)
}

/// Settings › Terminal › Test: run the Agent's version command (`claude
/// --version`) in `home` through `terminal` (spec §9.2).
pub fn test_terminal(
    agent: &dyn AgentAdapter,
    terminal: &Terminal,
    spawner: &dyn Spawner,
    home: &str,
) -> Result<(), LaunchError> {
    let command = agent.version_command(home)?;
    spawn_in(terminal, spawner, &command)
}

fn spawn_in(terminal: &Terminal, spawner: &dyn Spawner, command: &LaunchCommand) -> Result<(), LaunchError> {
    let wrapped = terminal.wrap(command);
    spawner
        .spawn(&wrapped)
        .map_err(|e| LaunchError::SpawnFailed { program: wrapped.program.clone(), os_error: e.to_string() })
}

/// A terminal-wrapped command, ready to spawn as an argv array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCommand {
    pub program: String,
    pub args: Vec<String>,
    /// The directory to start the process in; `None` inherits the app's.
    pub current_dir: Option<String>,
}

/// How a launch reaches a terminal: the presets from Settings (spec §7.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminal {
    /// `wt.exe -w new new-tab -d <cwd> <program> <args…>`.
    WindowsTerminalNewWindow,
    /// `wt.exe -w 0 new-tab -d <cwd> <program> <args…>`.
    WindowsTerminalNewTab,
    /// `cmd.exe /c start "" /D <cwd> <program> <args…>`.
    ConsoleWindow,
    /// The user's argv template (spec §9.2).
    Custom(CustomTemplate),
}

/// The placeholder replaced by the Workspace directory, as (part of) one argument.
pub const CWD_PLACEHOLDER: &str = "{cwd}";
/// The placeholder replaced by program + args, as separate arguments.
pub const COMMAND_PLACEHOLDER: &str = "{command}";

/// A validated custom terminal argv template: it has `{command}` as a whole
/// argument, so the Agent's command always reaches the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomTemplate(Vec<String>);

impl CustomTemplate {
    /// `Err` holds a human-readable reason, shown in Settings.
    pub fn new(argv: Vec<String>) -> Result<Self, String> {
        if argv.iter().any(|a| a != COMMAND_PLACEHOLDER && a.contains(COMMAND_PLACEHOLDER)) {
            return Err(format!("{COMMAND_PLACEHOLDER} must be an argument on its own"));
        }
        if !argv.iter().any(|a| a == COMMAND_PLACEHOLDER) {
            return Err(format!("The template must contain {COMMAND_PLACEHOLDER}"));
        }
        Ok(CustomTemplate(argv))
    }

    pub fn argv(&self) -> &[String] {
        &self.0
    }

    fn wrap(&self, cmd: &LaunchCommand) -> TerminalCommand {
        let mut argv = Vec::new();
        for part in &self.0 {
            if part == COMMAND_PLACEHOLDER {
                argv.push(cmd.program.clone());
                argv.extend(cmd.args.iter().cloned());
            } else {
                argv.push(part.replace(CWD_PLACEHOLDER, &cmd.cwd));
            }
        }
        // `new` guarantees `{command}`, so argv is never empty.
        let program = argv.remove(0);
        // Start in the directory too, for templates without `{cwd}`.
        TerminalCommand { program, args: argv, current_dir: Some(cmd.cwd.clone()) }
    }
}

impl Terminal {
    pub fn wrap(&self, cmd: &LaunchCommand) -> TerminalCommand {
        match self {
            Terminal::WindowsTerminalNewWindow => windows_terminal("new", cmd),
            Terminal::WindowsTerminalNewTab => windows_terminal("0", cmd),
            Terminal::ConsoleWindow => {
                // The first quoted argument to `start` is the window title, so
                // the empty title keeps a quoted program path from becoming it.
                let mut args = strings(&["/c", "start", "", "/D"]);
                // cmd expands `%NAME%` even inside quotes, so a directory with
                // a `%` never goes on its command line: cmd starts there
                // instead, and `start /D .` hands that directory on. A `%` in
                // the program or its arguments is not protected: going around
                // cmd's parsing would mean not using `cmd /c start` at all.
                let current_dir = if cmd.cwd.contains('%') {
                    args.push(".".into());
                    Some(cmd.cwd.clone())
                } else {
                    args.push(escape_for_cmd(&cmd.cwd));
                    None
                };
                args.push(escape_for_cmd(&cmd.program));
                args.extend(cmd.args.iter().map(|a| escape_for_cmd(a)));
                TerminalCommand { program: "cmd.exe".into(), args, current_dir }
            }
            Terminal::Custom(template) => template.wrap(cmd),
        }
    }
}

fn windows_terminal(window: &str, cmd: &LaunchCommand) -> TerminalCommand {
    let mut args = strings(&["-w", window, "new-tab", "-d"]);
    args.push(escape_for_wt(&cmd.cwd));
    args.push(escape_for_wt(&cmd.program));
    args.extend(cmd.args.iter().map(|a| escape_for_wt(a)));
    TerminalCommand { program: "wt.exe".into(), args, current_dir: None }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// `;` separates `wt` subcommands; a literal one is written `\;`.
fn escape_for_wt(arg: &str) -> String {
    arg.replace(';', r"\;")
}

/// `cmd` reparses its command line, so an unquoted `&`, `|`, `<`, `>` or `^`
/// (legal in a Windows user name, hence in an npm prefix) would split or
/// redirect it. An argument without whitespace reaches cmd unquoted, so
/// caret-escape those; one with whitespace is quoted, and inside quotes they
/// are literal. Each cmd layer strips one level, so nested layers compose.
pub(crate) fn escape_for_cmd(arg: &str) -> String {
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
