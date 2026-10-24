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
            Self::Internal(_) => {}
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
        std::process::Command::new(&cmd.program).args(&cmd.args).spawn().map(drop)
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
    let wrapped = terminal.wrap(&command);
    spawner
        .spawn(&wrapped)
        .map_err(|e| LaunchError::SpawnFailed { program: wrapped.program.clone(), os_error: e.to_string() })
}

/// A terminal-wrapped command, ready to spawn as an argv array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// How a launch reaches a terminal. Presets from Settings add variants here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    /// `wt.exe -w new new-tab -d <cwd> <program> <args…>`.
    WindowsTerminalNewWindow,
}

impl Terminal {
    pub fn wrap(&self, cmd: &LaunchCommand) -> TerminalCommand {
        match self {
            Terminal::WindowsTerminalNewWindow => {
                let mut args: Vec<String> =
                    ["-w", "new", "new-tab", "-d"].iter().map(|s| s.to_string()).collect();
                args.push(escape_for_wt(&cmd.cwd));
                args.push(escape_for_wt(&cmd.program));
                args.extend(cmd.args.iter().map(|a| escape_for_wt(a)));
                TerminalCommand { program: "wt.exe".into(), args }
            }
        }
    }
}

/// `;` separates `wt` subcommands; a literal one is written `\;`.
fn escape_for_wt(arg: &str) -> String {
    arg.replace(';', r"\;")
}
