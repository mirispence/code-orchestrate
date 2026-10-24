//! The Agent seam (spec §3.2): a compiled-in trait with one implementation per
//! Agent. An adapter describes itself and builds the command that starts it;
//! it never opens a terminal. Wrapping the command in a terminal is app code
//! (`crate::launch`).
//!
//! The third part of the contract, the Session scan, arrives with the scan
//! ticket.

pub mod claude_code;

use serde::Serialize;

use crate::paths::NormalizedPath;

/// What an Agent supports; the UI hides the matching controls when a flag is off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// "Continue last" (serialised as `continue`).
    #[serde(rename = "continue")]
    pub continue_last: bool,
    pub resume: bool,
    pub session_history: bool,
}

/// How a Session is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchMode {
    New,
    Continue,
    Resume(String),
}

/// A process to run, before any terminal wrapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum LaunchCommandError {
    /// The Agent's executable could not be resolved.
    #[error("{name} was not found")]
    ExecutableNotFound { name: String },
}

/// What the UI needs to show an Agent's launch controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub id: &'static str,
    pub display_name: &'static str,
    pub capabilities: Capabilities,
    /// The resolved executable, or `None` when it cannot be found.
    pub executable: Option<String>,
}

pub fn agent_status(agent: &dyn AgentAdapter) -> AgentStatus {
    AgentStatus {
        id: agent.id(),
        display_name: agent.display_name(),
        capabilities: agent.capabilities(),
        executable: agent.resolve_executable(),
    }
}

pub trait AgentAdapter: Send + Sync {
    /// Stable id, e.g. `claude-code`.
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    /// The Agent's executable, resolved now (never cached).
    fn resolve_executable(&self) -> Option<String>;
    /// The command that starts a Session in `workspace_path`. Resolves the
    /// executable afresh on every call.
    fn launch_command(
        &self,
        workspace_path: &NormalizedPath,
        mode: &LaunchMode,
    ) -> Result<LaunchCommand, LaunchCommandError>;
    /// A harmless command run in `cwd` (e.g. `claude --version`), used to
    /// test a terminal from Settings.
    fn version_command(&self, cwd: &str) -> Result<LaunchCommand, LaunchCommandError>;
}
