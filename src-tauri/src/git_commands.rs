//! The git status line and commands on a Workspace (spec §8.2.1). Every
//! command rechecks its conditions here when it is clicked, because the
//! buttons that started it can be stale.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agents::LaunchCommand;
use crate::launch::{self, LaunchError, Spawner, Terminal, escape_for_cmd};
use crate::launch_env::Environment;
use crate::repository::{Git, GitFailure, GitOutput, RepositoryError, find_remote};
use crate::store::Workspace;
use crate::ui_error::ui_error;

/// How long a command may run before it is stopped.
pub const COMMAND_LIMIT: Duration = Duration::from_secs(120);

/// A button on a Workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitCommand {
    Pull,
    Push,
    CheckoutDefault,
}

impl GitCommand {
    fn name(self) -> &'static str {
        match self {
            GitCommand::Pull => "Pull",
            GitCommand::Push => "Push",
            GitCommand::CheckoutDefault => "Checkout",
        }
    }
}

/// Why a command did not run or did not succeed. Serialises for the UI as
/// `{ kind, message, ...fields }`.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq, Serialize)]
#[serde(remote = "Self", tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GitCommandError {
    #[error("Workspace {workspace_id} does not exist")]
    WorkspaceNotFound { workspace_id: i64 },
    #[error("Folder not found: {path}")]
    WorkspaceMissing { workspace_id: i64, path: String },
    #[error("This folder is not in a git work tree, or git could not read it")]
    NotARepository,
    #[error("This repository has no remote to use")]
    NoRemote,
    #[error("HEAD is detached, so there is no branch to push")]
    DetachedHead,
    #[error("No default branch was found")]
    NoDefaultBranch,
    #[error("Already on {branch}")]
    AlreadyOnDefaultBranch { branch: String },
    #[error("There are uncommitted changes, so {branch} was not checked out. Commit or stash them first.")]
    UncommittedChanges { branch: String },
    #[error("Another git command is still running in this Workspace")]
    AlreadyRunning,
    /// git ran and reported this.
    #[error("{output}")]
    Failed { output: String },
    #[error("{command} took too long and was stopped")]
    TimedOut { command: String },
    #[error("{0}")]
    Terminal(#[serde(serialize_with = "crate::ui_error::omit")] String),
    #[error("{0}")]
    Internal(#[serde(serialize_with = "crate::ui_error::omit")] String),
}

ui_error!(GitCommandError);

impl From<LaunchError> for GitCommandError {
    fn from(e: LaunchError) -> Self {
        GitCommandError::Terminal(e.to_string())
    }
}

impl From<RepositoryError> for GitCommandError {
    fn from(e: RepositoryError) -> Self {
        match e {
            RepositoryError::WorkspaceNotFound { workspace_id } => GitCommandError::WorkspaceNotFound { workspace_id },
            other => GitCommandError::Internal(other.to_string()),
        }
    }
}

/// Ahead/behind counts against the upstream, as of the last contact with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Tracking {
    pub ahead: u32,
    pub behind: u32,
}

/// A Workspace's status line, and what its buttons depend on. Local only:
/// reading it never contacts the remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    /// `None` on a detached HEAD.
    pub branch: Option<String>,
    /// `None` when the branch has no upstream.
    pub tracking: Option<Tracking>,
    /// Staged, unstaged or untracked files.
    pub changes: bool,
    pub remote: Option<String>,
    pub default_branch: Option<String>,
}

/// The status, or `None` for a Missing Workspace (git is not run), outside a
/// work tree or when git can't run.
pub fn workspace_status(workspace: &Workspace, git: &dyn Git) -> Option<GitStatus> {
    if workspace.missing {
        return None;
    }
    Some(read_state(git, workspace.path.as_path())?.status)
}

/// A status with what a command needs beyond it.
struct RepositoryState {
    status: GitStatus,
    /// Whether the default branch exists locally.
    default_is_local: bool,
}

fn read_state(git: &dyn Git, dir: &Path) -> Option<RepositoryState> {
    // A background read must not take the index lock from an Agent's git, and
    // the Checkout check must not depend on the user's display preference.
    let args = ["--no-optional-locks", "status", "--porcelain=v2", "--branch", "--untracked-files=normal"];
    let stdout = git.run(dir, &args)?;
    let mut branch = None;
    let mut tracking = None;
    let mut changes = false;
    for line in stdout.lines() {
        if let Some(head) = line.strip_prefix("# branch.head ") {
            branch = (head != "(detached)").then(|| head.to_owned());
        } else if let Some(counts) = line.strip_prefix("# branch.ab ") {
            tracking = parse_counts(counts);
        } else if !line.starts_with('#') {
            changes = true;
        }
    }
    let remote = find_remote(git, dir).map(|remote| remote.name);
    let default = default_branch(git, dir, remote.as_deref());
    let default_is_local = default.as_ref().is_some_and(|d| d.local);
    let default_branch = default.map(|d| d.name);
    Some(RepositoryState { status: GitStatus { branch, tracking, changes, remote, default_branch }, default_is_local })
}

/// `+<ahead> -<behind>`.
fn parse_counts(counts: &str) -> Option<Tracking> {
    let (ahead, behind) = counts.split_once(' ')?;
    Some(Tracking { ahead: ahead.strip_prefix('+')?.parse().ok()?, behind: behind.strip_prefix('-')?.parse().ok()? })
}

struct DefaultBranch {
    name: String,
    local: bool,
}

/// The branch `<remote>/HEAD` points at, else `main`, else `master`: the first
/// that exists locally or on the remote.
fn default_branch(git: &dyn Git, dir: &Path, remote: Option<&str>) -> Option<DefaultBranch> {
    let stdout = git.run(dir, &["for-each-ref", "--format=%(refname) %(symref)", "refs/heads", "refs/remotes"])?;
    let refs: Vec<(&str, &str)> = stdout.lines().filter_map(|line| line.split_once(' ')).collect();
    let exists = |name: &str| refs.iter().any(|(reference, _)| *reference == name);
    let remote_prefix = remote.map(|remote| format!("refs/remotes/{remote}/"));
    let on_remote = |branch: &str| remote_prefix.as_ref().is_some_and(|prefix| exists(&format!("{prefix}{branch}")));

    let remote_head = remote_prefix.as_ref().and_then(|prefix| {
        let head = format!("{prefix}HEAD");
        let (_, target) = refs.iter().find(|(reference, _)| *reference == head)?;
        target.strip_prefix(prefix.as_str())
    });
    remote_head.into_iter().chain(["main", "master"]).find_map(|name| {
        let local = exists(&format!("refs/heads/{name}"));
        (local || on_remote(name)).then(|| DefaultBranch { name: name.to_owned(), local })
    })
}

/// The Workspaces with a command running: one at a time per Workspace.
#[derive(Debug, Default)]
pub struct RunningCommands(Mutex<HashSet<i64>>);

impl RunningCommands {
    /// Take the Workspace's slot until the returned value is dropped, or
    /// `None` when a command already holds it.
    pub fn start(&self, workspace_id: i64) -> Option<RunningCommand<'_>> {
        let mut running = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        running.insert(workspace_id).then(|| RunningCommand { running: self, workspace_id })
    }
}

/// Frees its Workspace's slot when dropped, however the command ended.
#[derive(Debug)]
pub struct RunningCommand<'a> {
    running: &'a RunningCommands,
    workspace_id: i64,
}

impl Drop for RunningCommand<'_> {
    fn drop(&mut self) {
        let mut running = self.running.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        running.remove(&self.workspace_id);
    }
}

/// The environment for a background command: a credential prompt in a terminal
/// nobody can see would hang it. Credential Manager's own window still works.
pub fn command_environment(launch_environment: Environment) -> Environment {
    let is_prompt = |name: &OsStr| name.eq_ignore_ascii_case("GIT_TERMINAL_PROMPT");
    let mut env: Environment = launch_environment.into_iter().filter(|(name, _)| !is_prompt(name)).collect();
    env.push(("GIT_TERMINAL_PROMPT".into(), "0".into()));
    env
}

/// Run `command` in the Workspace in the background, once its conditions hold.
pub fn run_command(
    workspace: &Workspace,
    git: &dyn Git,
    running: &RunningCommands,
    command: GitCommand,
) -> Result<(), GitCommandError> {
    let _slot = running.start(workspace.id).ok_or(GitCommandError::AlreadyRunning)?;
    let args = checked_args(workspace, git, command)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match git.execute(workspace.path.as_path(), &args, COMMAND_LIMIT) {
        Ok(output) if output.success => Ok(()),
        Ok(output) => Err(GitCommandError::Failed { output: failure_text(&output, &args) }),
        Err(GitFailure::TimedOut) => Err(GitCommandError::TimedOut { command: command.name().to_owned() }),
        Err(GitFailure::NotStarted(os_error)) => Err(GitCommandError::Failed { output: format!("Could not start git: {os_error}") }),
    }
}

/// What to show for a failed git: its error output, whatever it said.
fn failure_text(output: &GitOutput, args: &[&str]) -> String {
    [&output.stderr, &output.stdout]
        .iter()
        .map(|text| text.trim())
        .find(|text| !text.is_empty())
        .map_or_else(|| format!("git {} failed", args.join(" ")), str::to_owned)
}

/// Run the same command in the Workspace directory through `terminal`, where
/// the user can answer a login prompt.
pub fn run_in_terminal(
    workspace: &Workspace,
    git: &dyn Git,
    terminal: &Terminal,
    spawner: &dyn Spawner,
    env: &Environment,
    command: GitCommand,
) -> Result<(), GitCommandError> {
    let args = checked_args(workspace, git, command)?;
    Ok(launch::spawn_in(terminal, spawner, env, &terminal_command(args, workspace.path.as_str()))?)
}

/// `cmd /k` keeps the window open on the result, whichever terminal it's in.
fn terminal_command(args: Vec<String>, cwd: &str) -> LaunchCommand {
    let cwd = cwd.to_owned();
    if cfg!(windows) {
        let mut wrapped = vec!["/k".to_owned(), "git".to_owned()];
        wrapped.extend(args.iter().map(|arg| escape_for_cmd(arg)));
        LaunchCommand { program: "cmd".to_owned(), args: wrapped, cwd }
    } else {
        LaunchCommand { program: "git".to_owned(), args, cwd }
    }
}

/// The git arguments for `command`, or why it can't run now. The state is read
/// here, not taken from the buttons.
fn checked_args(workspace: &Workspace, git: &dyn Git, command: GitCommand) -> Result<Vec<String>, GitCommandError> {
    if workspace.missing {
        return Err(GitCommandError::WorkspaceMissing { workspace_id: workspace.id, path: workspace.path.to_string() });
    }
    let state = read_state(git, workspace.path.as_path()).ok_or(GitCommandError::NotARepository)?;
    let status = state.status;
    let strings = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect();
    match command {
        GitCommand::Pull => {
            status.remote.ok_or(GitCommandError::NoRemote)?;
            Ok(strings(&["pull", "--ff-only"]))
        }
        GitCommand::Push => {
            let remote = status.remote.ok_or(GitCommandError::NoRemote)?;
            status.branch.ok_or(GitCommandError::DetachedHead)?;
            Ok(strings(&["push", "-u", &remote, "HEAD"]))
        }
        GitCommand::CheckoutDefault => {
            let branch = status.default_branch.ok_or(GitCommandError::NoDefaultBranch)?;
            if status.branch.as_ref() == Some(&branch) {
                return Err(GitCommandError::AlreadyOnDefaultBranch { branch });
            }
            if status.changes {
                return Err(GitCommandError::UncommittedChanges { branch });
            }
            if state.default_is_local {
                return Ok(strings(&["switch", &branch]));
            }
            // Named in full: `git switch <name>` won't guess with two remotes.
            let remote = status.remote.ok_or(GitCommandError::NoDefaultBranch)?;
            Ok(strings(&["switch", "--create", &branch, "--track", &format!("{remote}/{branch}")]))
        }
    }
}
