//! The Agent seam (spec §3.2): a compiled-in trait with one implementation per
//! Agent. An adapter describes itself and builds the command that starts it;
//! it never opens a terminal. Wrapping the command in a terminal is app code
//! (`crate::launch`).
//!
//! The third part, the Session scan, lists the adapter's transcript files
//! with their source keys and parses one file into a [`SessionSummary`]. All
//! transcript-format knowledge stays inside the adapter.

pub mod claude_code;

use std::path::{Path, PathBuf};

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

/// A transcript file's identity at one moment (spec §5.1): the scan re-parses
/// a file only when this differs from the stored SourceFile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceKey {
    pub path: PathBuf,
    /// Modification time, nanoseconds since the Unix epoch.
    pub mtime: i64,
    pub size: u64,
}

/// One Session as parsed from its transcript (spec §3.2). Times are Unix
/// epoch milliseconds, UTC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub session_id: String,
    pub first_cwd: NormalizedPath,
    pub started_at: Option<i64>,
    pub last_activity_at: Option<i64>,
    pub source_path: PathBuf,
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
    /// Every transcript file in the Agent's store, reading metadata only.
    /// An unreadable or missing store yields none.
    fn list_sources(&self) -> Vec<SourceKey>;
    /// Parse one transcript. `Ok(None)` when it holds no Session yet (no
    /// record with a working directory); `Err` when it cannot be read now, so
    /// the scan retries it next time.
    fn parse_source(&self, path: &Path) -> std::io::Result<Option<SessionSummary>>;
}
