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

/// How a Session is started. The UI sends `"new"`, `"continue"`, or
/// `{ "resume": "<session id>" }`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// A display title, if the transcript offers one.
    pub title: Option<String>,
    pub started_at: Option<i64>,
    pub last_activity_at: Option<i64>,
    /// Time spent active, in milliseconds; `None` when the Agent does not report it.
    pub active_time_ms: Option<i64>,
    pub tokens: TokenUsage,
    pub source_path: PathBuf,
}

/// Token counts by kind. A field is `None` when the Agent does not report it
/// (shown as "—", never 0).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub cache_write: Option<i64>,
    pub cache_read: Option<i64>,
}

impl TokenUsage {
    /// Every field reported, all zero.
    pub const ZERO: TokenUsage =
        TokenUsage { input: Some(0), output: Some(0), cache_write: Some(0), cache_read: Some(0) };

    /// Field-wise sum. A field is `None` only when both sides lack it.
    pub fn plus(self, other: TokenUsage) -> TokenUsage {
        fn add(a: Option<i64>, b: Option<i64>) -> Option<i64> {
            match (a, b) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
            }
        }
        TokenUsage {
            input: add(self.input, other.input),
            output: add(self.output, other.output),
            cache_write: add(self.cache_write, other.cache_write),
            cache_read: add(self.cache_read, other.cache_read),
        }
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
    /// Every transcript file in the Agent's store, reading metadata only.
    /// An unreadable or missing store yields none.
    fn list_sources(&self) -> Vec<SourceKey>;
    /// Parse one transcript. `Ok(None)` when it holds no Session yet (no
    /// record with a working directory); `Err` when it cannot be read now, so
    /// the scan retries it next time.
    fn parse_source(&self, path: &Path) -> std::io::Result<Option<SessionSummary>>;
    /// A harmless command run in `cwd` (e.g. `claude --version`), used to
    /// test a terminal from Settings.
    fn version_command(&self, cwd: &str) -> Result<LaunchCommand, LaunchCommandError>;
}
