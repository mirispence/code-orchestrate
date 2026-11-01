//! The git status line and commands on a Workspace (spec §8.2.1): the logic
//! against canned git output, then the behaviours git itself decides.

use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use code_orchestrate_lib::git_commands::{
    COMMAND_LIMIT, GitCommand, GitCommandError, GitStatus, RunningCommands, Tracking, command_environment,
    run_command, run_in_terminal, workspace_status,
};
use code_orchestrate_lib::launch::{Spawner, Terminal, TerminalCommand};
use code_orchestrate_lib::paths::normalize;
use code_orchestrate_lib::repository::{Git, GitFailure, GitOutput, OsGit};
use code_orchestrate_lib::store::Workspace;

/// Answers each `git` call from a canned list, by the start of its arguments,
/// and records the calls. A call nothing answers fails like git would.
#[derive(Default)]
struct FakeGit {
    answers: Vec<(&'static str, Result<GitOutput, GitFailure>)>,
    calls: RefCell<Vec<(String, Duration)>>,
}

impl FakeGit {
    fn answering(mut self, args: &'static str, stdout: &str) -> Self {
        self.answers.push((args, Ok(GitOutput::succeeded(stdout))));
        self
    }

    fn failing(mut self, args: &'static str, stderr: &str) -> Self {
        self.answers.push((args, Ok(GitOutput::failed(stderr))));
        self
    }

    fn timing_out(mut self, args: &'static str) -> Self {
        self.answers.push((args, Err(GitFailure::TimedOut)));
        self
    }

    /// The calls that change the repository, not the reads before them.
    fn changes(&self) -> Vec<String> {
        let reads = ["remote -v", "--no-optional-locks status", "for-each-ref"];
        let calls = self.calls.borrow();
        calls.iter().map(|(args, _)| args.clone()).filter(|a| !reads.iter().any(|r| a.starts_with(r))).collect()
    }
}

impl Git for FakeGit {
    fn execute(&self, _dir: &Path, args: &[&str], limit: Duration) -> Result<GitOutput, GitFailure> {
        let call = args.join(" ");
        self.calls.borrow_mut().push((call.clone(), limit));
        let answer = self.answers.iter().find(|(start, _)| call.starts_with(start));
        answer.map_or_else(|| Ok(GitOutput::failed("fatal: unexpected")), |(_, result)| result.clone())
    }
}

const ORIGIN: &str = "origin\tgit@github.com:owner/repo.git (fetch)\norigin\tgit@github.com:owner/repo.git (push)\n";
const CLEAN_FEATURE: &str =
    "# branch.oid abc\n# branch.head feature\n# branch.upstream origin/feature\n# branch.ab +2 -3\n";
const DETACHED: &str = "# branch.oid abc\n# branch.head (detached)\n";
const NO_UPSTREAM: &str = "# branch.oid abc\n# branch.head feature\n";
const CHANGED: &str = "# branch.oid abc\n# branch.head feature\n1 .M N... 100644 100644 100644 a b src/app.rs\n";
const UNTRACKED: &str = "# branch.oid abc\n# branch.head feature\n? notes.txt\n";
const ON_MAIN: &str = "# branch.oid abc\n# branch.head main\n";
const REFS_LOCAL_MAIN: &str = "refs/heads/feature \nrefs/heads/main \n";

fn status_call(status: &str) -> FakeGit {
    FakeGit::default().answering("--no-optional-locks status", status)
}

/// A repository with `origin`, on a clean `feature` with a local `main`.
fn clean_repo() -> FakeGit {
    status_call(CLEAN_FEATURE).answering("remote -v", ORIGIN).answering("for-each-ref", REFS_LOCAL_MAIN)
}

fn workspace(missing: bool) -> Workspace {
    Workspace {
        id: 7,
        name: "App".into(),
        path: normalize(r"E:\src\app").unwrap(),
        pinned: false,
        note: None,
        created_at: 0,
        tags: vec![],
        former_paths: vec![],
        missing,
    }
}

fn status_of(git: &FakeGit) -> Option<GitStatus> {
    workspace_status(&workspace(false), git)
}

fn run(git: &FakeGit, command: GitCommand) -> Result<(), GitCommandError> {
    run_command(&workspace(false), git, &RunningCommands::default(), command)
}

// The status line.

#[test]
fn status_has_the_branch_counts_and_the_commands_conditions() {
    assert_eq!(
        status_of(&clean_repo()),
        Some(GitStatus {
            branch: Some("feature".into()),
            tracking: Some(Tracking { ahead: 2, behind: 3 }),
            changes: false,
            remote: Some("origin".into()),
            default_branch: Some("main".into()),
        })
    );
}

#[test]
fn a_detached_head_has_no_branch() {
    let status = status_of(&status_call(DETACHED)).unwrap();
    assert_eq!((status.branch, status.tracking), (None, None));
}

#[test]
fn a_branch_with_no_upstream_has_no_counts() {
    let status = status_of(&status_call(NO_UPSTREAM)).unwrap();
    assert_eq!((status.branch.as_deref(), status.tracking), (Some("feature"), None));
}

#[test]
fn modified_and_untracked_files_are_uncommitted_changes() {
    assert!(!status_of(&status_call(NO_UPSTREAM)).unwrap().changes);
    assert!(status_of(&status_call(CHANGED)).unwrap().changes);
    assert!(status_of(&status_call(UNTRACKED)).unwrap().changes);
}

#[test]
fn a_repository_without_a_remote_has_a_status_and_no_remote() {
    let git = status_call(NO_UPSTREAM).answering("remote -v", "").answering("for-each-ref", REFS_LOCAL_MAIN);
    let status = status_of(&git).unwrap();
    assert_eq!((status.remote, status.default_branch.as_deref()), (None, Some("main")));
}

#[test]
fn outside_a_work_tree_there_is_no_status() {
    let git = FakeGit::default().failing("--no-optional-locks status", "fatal: not a git repository");
    assert_eq!(status_of(&git), None);
}

#[test]
fn a_missing_workspace_has_no_status_and_git_does_not_run() {
    let git = FakeGit::default();
    assert_eq!(workspace_status(&workspace(true), &git), None);
    assert!(git.calls.borrow().is_empty());
}

#[test]
fn status_reads_keep_the_short_limit() {
    let git = clean_repo();
    status_of(&git);
    assert!(git.calls.borrow().iter().all(|(_, limit)| *limit == Duration::from_secs(5)));
}

// The default branch.

fn default_branch(refs: &str) -> Option<String> {
    let git = status_call(NO_UPSTREAM).answering("remote -v", ORIGIN).answering("for-each-ref", refs);
    status_of(&git).unwrap().default_branch
}

#[test]
fn origin_head_decides_the_default_branch() {
    let refs = "refs/heads/main \nrefs/remotes/origin/HEAD refs/remotes/origin/trunk\nrefs/remotes/origin/trunk \n";
    assert_eq!(default_branch(refs).as_deref(), Some("trunk"));
}

#[test]
fn main_comes_before_master_and_either_can_be_on_the_remote() {
    assert_eq!(default_branch("refs/heads/master \nrefs/heads/main \n").as_deref(), Some("main"));
    assert_eq!(default_branch("refs/heads/master \nrefs/remotes/origin/main \n").as_deref(), Some("main"));
    assert_eq!(default_branch("refs/remotes/origin/master \n").as_deref(), Some("master"));
}

#[test]
fn a_repository_with_neither_has_no_default_branch() {
    assert_eq!(default_branch("refs/heads/develop \n"), None);
}

#[test]
fn origin_head_pointing_at_a_deleted_branch_falls_through() {
    let refs = "refs/heads/main \nrefs/remotes/origin/HEAD refs/remotes/origin/gone\n";
    assert_eq!(default_branch(refs).as_deref(), Some("main"));
}

// Pull.

#[test]
fn pull_only_fast_forwards_under_the_long_limit() {
    let git = clean_repo().answering("pull", "");
    run(&git, GitCommand::Pull).unwrap();
    assert_eq!(git.changes(), ["pull --ff-only"]);
    assert!(git.calls.borrow().contains(&("pull --ff-only".to_owned(), COMMAND_LIMIT)));
    assert_eq!(COMMAND_LIMIT, Duration::from_secs(120));
}

#[test]
fn pull_is_refused_when_the_remote_has_gone_since_the_buttons_refreshed() {
    let git = status_call(CLEAN_FEATURE).answering("remote -v", "");
    assert!(matches!(run(&git, GitCommand::Pull), Err(GitCommandError::NoRemote)));
    assert_eq!(git.changes(), Vec::<String>::new());
}

#[test]
fn a_failed_command_reports_gits_error_output() {
    let git = clean_repo().failing("pull", "fatal: Not possible to fast-forward, aborting.\n");
    let error = run(&git, GitCommand::Pull).unwrap_err();
    assert_eq!(error.to_string(), "fatal: Not possible to fast-forward, aborting.");
}

#[test]
fn a_command_that_runs_out_of_time_is_reported_as_failed() {
    let git = clean_repo().timing_out("pull");
    assert!(matches!(run(&git, GitCommand::Pull), Err(GitCommandError::TimedOut { .. })));
}

// Push.

#[test]
fn push_sets_the_upstream_on_origin_for_the_current_branch() {
    let git = clean_repo().answering("push", "");
    run(&git, GitCommand::Push).unwrap();
    assert_eq!(git.changes(), ["push -u origin HEAD"]);
}

#[test]
fn push_uses_the_only_remote_when_there_is_no_origin() {
    let git = status_call(CLEAN_FEATURE)
        .answering("remote -v", "fork\tgit@github.com:me/repo.git (fetch)\n")
        .answering("for-each-ref", REFS_LOCAL_MAIN)
        .answering("push", "");
    run(&git, GitCommand::Push).unwrap();
    assert_eq!(git.changes(), ["push -u fork HEAD"]);
}

#[test]
fn push_is_refused_on_a_detached_head() {
    let git = status_call(DETACHED).answering("remote -v", ORIGIN).answering("for-each-ref", REFS_LOCAL_MAIN);
    assert!(matches!(run(&git, GitCommand::Push), Err(GitCommandError::DetachedHead)));
    assert_eq!(git.changes(), Vec::<String>::new());
}

// Checkout default branch.

#[test]
fn checkout_switches_to_the_local_default_branch_and_does_not_pull() {
    let git = clean_repo().answering("switch", "");
    run(&git, GitCommand::CheckoutDefault).unwrap();
    assert_eq!(git.changes(), ["switch main"]);
}

#[test]
fn checkout_creates_a_tracking_branch_when_only_the_remote_one_exists() {
    let git = status_call(CLEAN_FEATURE)
        .answering("remote -v", ORIGIN)
        .answering("for-each-ref", "refs/heads/feature \nrefs/remotes/origin/main \n")
        .answering("switch", "");
    run(&git, GitCommand::CheckoutDefault).unwrap();
    assert_eq!(git.changes(), ["switch --create main --track origin/main"]);
}

/// The status is read on the click, so changes made since the buttons last
/// refreshed count too.
#[test]
fn checkout_refuses_with_uncommitted_changes_and_changes_nothing() {
    let git = status_call(CHANGED).answering("remote -v", ORIGIN).answering("for-each-ref", REFS_LOCAL_MAIN);
    let error = run(&git, GitCommand::CheckoutDefault).unwrap_err();
    assert!(matches!(error, GitCommandError::UncommittedChanges { .. }), "{error:?}");
    assert!(error.to_string().contains("main"));
    assert_eq!(git.changes(), Vec::<String>::new());
}

#[test]
fn checkout_is_refused_when_already_on_the_default_branch() {
    let git = status_call(ON_MAIN).answering("remote -v", ORIGIN).answering("for-each-ref", REFS_LOCAL_MAIN);
    assert!(matches!(run(&git, GitCommand::CheckoutDefault), Err(GitCommandError::AlreadyOnDefaultBranch { .. })));
    assert_eq!(git.changes(), Vec::<String>::new());
}

#[test]
fn checkout_is_refused_without_a_default_branch() {
    let git = status_call(CLEAN_FEATURE).answering("remote -v", ORIGIN).answering("for-each-ref", "refs/heads/dev \n");
    assert!(matches!(run(&git, GitCommand::CheckoutDefault), Err(GitCommandError::NoDefaultBranch)));
}

// Across commands.

#[test]
fn commands_are_refused_for_a_missing_workspace_outside_a_work_tree() {
    let git = FakeGit::default();
    let missing = run_command(&workspace(true), &git, &RunningCommands::default(), GitCommand::Pull);
    assert!(matches!(missing, Err(GitCommandError::WorkspaceMissing { .. })));
    assert!(git.calls.borrow().is_empty());

    let outside = FakeGit::default().failing("--no-optional-locks status", "fatal: not a git repository");
    assert!(matches!(run(&outside, GitCommand::Pull), Err(GitCommandError::NotARepository)));
}

#[test]
fn one_command_runs_at_a_time_per_workspace() {
    let running = RunningCommands::default();
    let git = clean_repo();
    let held = running.start(7).unwrap();

    let refused = run_command(&workspace(false), &git, &running, GitCommand::Pull);
    assert!(matches!(refused, Err(GitCommandError::AlreadyRunning)));
    assert!(running.start(8).is_some(), "another Workspace stays usable");

    drop(held);
    assert!(running.start(7).is_some(), "finishing frees the Workspace");
}

#[test]
fn the_slot_is_free_again_after_a_failed_command() {
    let running = RunningCommands::default();
    let git = clean_repo().failing("pull", "fatal: no");
    run_command(&workspace(false), &git, &running, GitCommand::Pull).unwrap_err();
    assert!(running.start(7).is_some());
}

#[test]
fn background_commands_never_prompt_in_a_terminal() {
    let env = command_environment(vec![("GIT_TERMINAL_PROMPT".into(), "1".into()), ("X".into(), "1".into())]);
    let prompts: Vec<&OsString> = env.iter().filter(|(k, _)| k == "GIT_TERMINAL_PROMPT").map(|(_, v)| v).collect();
    assert_eq!(prompts, ["0"]);
    assert!(env.iter().any(|(k, _)| k == "X"));
}

// Run in terminal.

#[derive(Default)]
struct FakeSpawner {
    spawned: RefCell<Vec<TerminalCommand>>,
}

impl Spawner for FakeSpawner {
    fn spawn(&self, cmd: &TerminalCommand) -> io::Result<()> {
        self.spawned.borrow_mut().push(cmd.clone());
        Ok(())
    }
}

#[test]
fn run_in_terminal_opens_the_same_command_in_the_workspace_with_the_launch_environment() {
    let git = clean_repo();
    let spawner = FakeSpawner::default();
    let env = vec![("MY_OWN".into(), "1".into())];
    let terminal = Terminal::Custom(
        code_orchestrate_lib::launch::CustomTemplate::new(vec!["term".into(), "{cwd}".into(), "{command}".into()]).unwrap(),
    );

    run_in_terminal(&workspace(false), &git, &terminal, &spawner, &env, GitCommand::Push).unwrap();

    let spawned = spawner.spawned.into_inner();
    let args: Vec<&str> = spawned[0].args.iter().map(String::as_str).collect();
    assert_eq!(spawned[0].program, "term");
    assert_eq!(args[0], r"E:\src\app");
    assert!(args.ends_with(&["git", "push", "-u", "origin", "HEAD"]), "{args:?}");
    assert_eq!(spawned[0].env, Some(env));
    assert_eq!(git.changes(), Vec::<String>::new());
}

// Real git: what git itself decides.

struct Repo(tempfile::TempDir);

fn git_in(dir: &Path, args: &[&str]) {
    let output = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
}

impl Repo {
    /// A bare `origin` with one commit on `main`, and a clone at `work`.
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_in(root, &["init", "--bare", "--initial-branch=main", "origin.git"]);
        git_in(root, &["clone", "origin.git", "work"]);
        let repo = Repo(dir);
        repo.configure("work");
        repo.commit("work", "first");
        git_in(&repo.work(), &["push", "origin", "HEAD:main"]);
        repo
    }

    fn work(&self) -> std::path::PathBuf {
        self.0.path().join("work")
    }

    fn configure(&self, clone: &str) {
        let dir = self.0.path().join(clone);
        git_in(&dir, &["config", "user.email", "test@example.com"]);
        git_in(&dir, &["config", "user.name", "Test"]);
        git_in(&dir, &["config", "core.autocrlf", "false"]);
    }

    fn commit(&self, clone: &str, file: &str) {
        let dir = self.0.path().join(clone);
        std::fs::write(dir.join(file), file).unwrap();
        git_in(&dir, &["add", "."]);
        git_in(&dir, &["commit", "-m", file]);
    }

    fn head(&self) -> String {
        let output = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(self.work()).output().unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn workspace(&self) -> Workspace {
        Workspace { path: normalize(self.work().to_str().unwrap()).unwrap(), ..workspace(false) }
    }

    fn run(&self, command: GitCommand) -> Result<(), GitCommandError> {
        run_command(&self.workspace(), &OsGit::default(), &RunningCommands::default(), command)
    }

    fn status(&self) -> GitStatus {
        workspace_status(&self.workspace(), &OsGit::default()).unwrap()
    }
}

#[test]
fn pull_on_a_diverged_branch_fails_and_changes_nothing() {
    let repo = Repo::new();
    git_in(repo.0.path(), &["clone", "origin.git", "other"]);
    repo.configure("other");
    repo.commit("other", "theirs");
    git_in(&repo.0.path().join("other"), &["push", "origin", "HEAD:main"]);
    repo.commit("work", "ours");
    let before = repo.head();

    let error = repo.run(GitCommand::Pull).unwrap_err();

    assert!(matches!(error, GitCommandError::Failed { .. }), "{error:?}");
    assert_eq!(repo.head(), before);
}

#[test]
fn push_sets_the_upstream_on_a_new_branch() {
    let repo = Repo::new();
    git_in(&repo.work(), &["switch", "-c", "feature"]);
    repo.commit("work", "more");
    assert_eq!(repo.status().tracking, None);

    repo.run(GitCommand::Push).unwrap();

    assert_eq!(repo.status().tracking, Some(Tracking { ahead: 0, behind: 0 }));
}

#[test]
fn checkout_tracks_the_remote_branch_when_there_is_no_local_one() {
    let repo = Repo::new();
    git_in(&repo.work(), &["switch", "-c", "feature"]);
    git_in(&repo.work(), &["branch", "-D", "main"]);

    repo.run(GitCommand::CheckoutDefault).unwrap();

    let status = repo.status();
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.tracking, Some(Tracking { ahead: 0, behind: 0 }));
}

#[test]
fn untracked_files_count_even_when_git_is_set_to_hide_them() {
    let repo = Repo::new();
    git_in(&repo.work(), &["config", "status.showUntrackedFiles", "no"]);
    std::fs::write(repo.work().join("notes.txt"), "x").unwrap();
    assert!(repo.status().changes);
}

#[test]
fn a_repository_with_no_commits_still_has_a_status() {
    let dir = tempfile::tempdir().unwrap();
    git_in(dir.path(), &["init", "--initial-branch=main"]);
    let workspace = Workspace { path: normalize(dir.path().to_str().unwrap()).unwrap(), ..workspace(false) };
    let status = workspace_status(&workspace, &OsGit::default()).unwrap();
    assert_eq!(status.branch.as_deref(), Some("main"));
}

/// A helper that outlives `git` keeps the pipes open, so this also shows the
/// caller isn't left waiting on them.
#[test]
fn a_hung_git_is_stopped_at_its_limit() {
    let dir = tempfile::tempdir().unwrap();
    let args = ["-c", "alias.hang=!sleep 30", "hang"];
    let started = Instant::now();
    let result = OsGit::default().execute(dir.path(), &args, Duration::from_millis(300));
    assert_eq!(result, Err(GitFailure::TimedOut));
    assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
}

/// A survivor could change the repository after the UI said it had stopped.
#[cfg(windows)]
#[test]
fn stopping_a_hung_git_stops_what_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let args = ["-c", "alias.hang=!sleep 2; touch survived", "hang"];
    let result = OsGit::default().execute(dir.path(), &args, Duration::from_millis(500));
    assert_eq!(result, Err(GitFailure::TimedOut));
    std::thread::sleep(Duration::from_secs(3));
    assert!(!dir.path().join("survived").exists());
}

#[test]
fn stderr_and_the_exit_status_come_back_from_a_failed_git() {
    let dir = tempfile::tempdir().unwrap();
    let output = OsGit::default().execute(dir.path(), &["no-such-command"], Duration::from_secs(5)).unwrap();
    assert!(!output.success);
    assert!(output.stderr.contains("no-such-command"), "{output:?}");
}
