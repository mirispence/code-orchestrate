//! A Workspace's git repository: its remote, read by running `git`, turned
//! into the repository's web page.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::launch::{self, Spawner};
use crate::store::{Store, StoreError, Workspace};
use crate::ui_error::ui_error;

/// Runs `git`. The real one is [`OsGit`]; tests answer with canned output.
pub trait Git {
    /// `git <args>` in `dir`: its stdout, or `None` when it could not run or failed.
    fn run(&self, dir: &Path, args: &[&str]) -> Option<String>;
}

/// The repository whose remote `dir` (or a folder above it) has: `origin`, or
/// the only remote when there is no `origin`. `None` when there is none, the
/// folder is not in a repository or git cannot run.
pub fn find(git: &dyn Git, dir: &Path) -> Option<Repository> {
    // Lines are `name<TAB>url (fetch)` and `name<TAB>url (push)`; URLs have no spaces.
    let stdout = git.run(dir, &["remote", "-v"])?;
    let mut remotes: Vec<(&str, &str)> = Vec::new();
    for line in stdout.lines() {
        let Some((name, rest)) = line.split_once('\t') else { continue };
        // A partial clone adds its filter after it: `(fetch) [blob:none]`.
        let Some((url, _)) = rest.split_once(" (fetch)") else { continue };
        remotes.push((name, url));
    }
    let url = match remotes.iter().find(|(name, _)| *name == "origin") {
        Some((_, url)) => url,
        None if remotes.len() == 1 => remotes[0].1,
        None => return None,
    };
    web_page(url)
}

/// Why a repository page could not be opened. Serialises for the UI as
/// `{ kind, message, ...fields }`.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq, Serialize)]
#[serde(remote = "Self", tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RepositoryError {
    #[error("Workspace {workspace_id} does not exist")]
    WorkspaceNotFound { workspace_id: i64 },
    #[error("Folder not found: {path}")]
    WorkspaceMissing { workspace_id: i64, path: String },
    /// The remote is gone or changed to one with no page since it was shown.
    #[error("This folder no longer has a git remote with a web page")]
    NoRepository,
    #[error("Could not open the repository page: {os_error}")]
    OpenFailed { os_error: String },
    #[error("{0}")]
    Internal(#[serde(serialize_with = "crate::ui_error::omit")] String),
}

ui_error!(RepositoryError);

/// The Workspace, read (and its `missing` checked) now. Callers run git after
/// letting go of the store, so a slow git never holds it.
pub fn get_workspace(store: &Store, workspace_id: i64) -> Result<Workspace, RepositoryError> {
    store.get_workspace(workspace_id).map_err(|e| match e {
        StoreError::NoSuchWorkspace(_) => RepositoryError::WorkspaceNotFound { workspace_id },
        other => RepositoryError::Internal(other.to_string()),
    })
}

/// The Workspace's repository for the detail header, or `None` (also for a
/// Missing Workspace, where git is not run).
pub fn workspace_repository(workspace: &Workspace, git: &dyn Git) -> Option<Repository> {
    if workspace.missing {
        return None;
    }
    find(git, workspace.path.as_path())
}

/// Open the Workspace's repository page in the default browser. The URL is
/// resolved here, never taken from the webview.
pub fn open_workspace_repository(
    workspace: &Workspace,
    git: &dyn Git,
    spawner: &dyn Spawner,
) -> Result<(), RepositoryError> {
    if workspace.missing {
        return Err(RepositoryError::WorkspaceMissing {
            workspace_id: workspace.id,
            path: workspace.path.to_string(),
        });
    }
    let repository = find(git, workspace.path.as_path()).ok_or(RepositoryError::NoRepository)?;
    launch::open_with_system(spawner, &repository.url)
        .map_err(|e| RepositoryError::OpenFailed { os_error: e.to_string() })
}

/// How long `git` may take before it is given up on.
const GIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs `git` from PATH without a console window, giving up after [`GIT_TIMEOUT`].
pub struct OsGit;

impl Git for OsGit {
    fn run(&self, dir: &Path, args: &[&str]) -> Option<String> {
        let mut command = Command::new("git");
        command.args(args).current_dir(dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().ok()?;
        // Read stdout while waiting, so a full pipe cannot stall git.
        let mut stdout = child.stdout.take()?;
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            stdout.read_to_string(&mut text).ok().map(|_| text)
        });
        let deadline = Instant::now() + GIT_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        };
        let text = reader.join().ok()??;
        status.success().then_some(text)
    }
}

/// A repository's web page, e.g. `owner/repo` at `https://github.com/owner/repo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Repository {
    pub name: String,
    pub url: String,
}

/// The web page for a remote URL, or `None` for one with no page (a local
/// path, `file://`, an unknown scheme). Credentials are dropped.
pub fn web_page(remote: &str) -> Option<Repository> {
    let remote = remote.trim();
    if let Some(rest) = remote.strip_prefix("https://") {
        let (authority, path) = rest.split_once('/')?;
        return page_at(without_user(authority), path);
    }
    if let Some(rest) = remote.strip_prefix("ssh://") {
        let (authority, path) = rest.split_once('/')?;
        // The SSH port is not the web server's.
        let host = without_user(authority);
        let host = host.split_once(':').map_or(host, |(host, _port)| host);
        return page_at(host, path);
    }
    let (host, path) = scp_like(remote)?;
    page_at(host, path)
}

/// `[user@]host:path`, git's shorthand for SSH. Like git, a slash before the
/// colon makes it a local path instead.
fn scp_like(remote: &str) -> Option<(&str, &str)> {
    if remote.contains("://") {
        return None;
    }
    let (authority, path) = remote.split_once(':')?;
    if authority.contains(['/', '\\']) {
        return None;
    }
    Some((without_user(authority), path))
}

/// The authority with any `user[:password]@` taken off.
fn without_user(authority: &str) -> &str {
    authority.rsplit_once('@').map_or(authority, |(_, host)| host)
}

/// The page for `path` on `host` (`name[:port]`). The path must be
/// `owner/repo` or deeper. Only plain host and path characters are accepted,
/// since the URL is handed to the system opener as an argument.
fn page_at(host: &str, path: &str) -> Option<Repository> {
    let (name, port) = host.split_once(':').map_or((host, None), |(name, port)| (name, Some(port)));
    // A host without a dot is a drive letter, `localhost` or an SSH alias
    // from `~/.ssh/config`, none of which has a page under that name.
    let host_ok = name.contains('.')
        && name.split('.').all(|label| !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let port_ok = port.is_none_or(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if !host_ok || !port_ok {
        return None;
    }
    let path = path.trim_start_matches('/');
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = path.strip_suffix(".git").unwrap_or(path);
    let segments: Vec<&str> = path.split('/').collect();
    let segment_ok = |s: &&str| {
        !s.is_empty() && *s != "." && *s != ".." && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.~".contains(c))
    };
    if segments.len() < 2 || !segments.iter().all(segment_ok) {
        return None;
    }
    match name {
        // Azure DevOps SSH paths are `v3/org/project/repo`; the page is
        // `dev.azure.com/org/project/_git/repo`.
        "ssh.dev.azure.com" => match segments[..] {
            ["v3", org, project, repo] => Some(Repository {
                name: format!("{org}/{project}/{repo}"),
                url: format!("https://dev.azure.com/{org}/{project}/_git/{repo}"),
            }),
            _ => None,
        },
        // Its older SSH host: the page's host depends on the organisation.
        "vs-ssh.visualstudio.com" => None,
        _ => Some(Repository { name: path.to_owned(), url: format!("https://{host}/{path}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers every call with `stdout`, or fails like a missing `git` or a
    /// folder outside any repository when it is `None`.
    struct FakeGit(Option<&'static str>);

    impl Git for FakeGit {
        fn run(&self, _dir: &Path, _args: &[&str]) -> Option<String> {
            self.0.map(str::to_owned)
        }
    }

    fn found(stdout: Option<&'static str>) -> Option<String> {
        find(&FakeGit(stdout), Path::new("E:/src/app")).map(|r| r.url)
    }

    #[test]
    fn origin_is_used_when_there_are_several_remotes() {
        let stdout = "fork\tgit@github.com:me/app.git (fetch)\nfork\tgit@github.com:me/app.git (push)\n\
                      origin\tgit@github.com:team/app.git (fetch)\norigin\tgit@github.com:team/app.git (push)\n";
        assert_eq!(found(Some(stdout)), Some("https://github.com/team/app".to_owned()));
    }

    #[test]
    fn the_only_remote_is_used_when_there_is_no_origin() {
        let stdout = "upstream\thttps://github.com/team/app.git (fetch)\nupstream\thttps://github.com/team/app.git (push)\n";
        assert_eq!(found(Some(stdout)), Some("https://github.com/team/app".to_owned()));
    }

    /// A partial clone annotates its fetch line with the filter.
    #[test]
    fn origin_is_found_in_a_partial_clone() {
        let stdout = "fork\tgit@github.com:me/app.git (fetch)\nfork\tgit@github.com:me/app.git (push)\n\
                      origin\tgit@github.com:team/app.git (fetch) [blob:none]\norigin\tgit@github.com:team/app.git (push)\n";
        assert_eq!(found(Some(stdout)), Some("https://github.com/team/app".to_owned()));
    }

    #[test]
    fn several_remotes_without_origin_show_nothing() {
        let stdout = "a\tgit@github.com:me/app.git (fetch)\nb\tgit@github.com:team/app.git (fetch)\n";
        assert_eq!(found(Some(stdout)), None);
    }

    #[test]
    fn no_remote_no_repository_or_no_git_shows_nothing() {
        assert_eq!(found(Some("")), None);
        assert_eq!(found(None), None);
    }

    fn page(remote: &str) -> Option<(String, String)> {
        web_page(remote).map(|r| (r.name, r.url))
    }

    fn github(name: &str) -> Option<(String, String)> {
        Some((name.to_owned(), format!("https://github.com/{name}")))
    }

    #[test]
    fn scp_style_ssh_remote_becomes_https_page() {
        assert_eq!(page("git@github.com:owner/repo.git"), github("owner/repo"));
    }

    #[test]
    fn https_remote_keeps_its_page_with_or_without_dot_git() {
        assert_eq!(page("https://github.com/owner/repo.git"), github("owner/repo"));
        assert_eq!(page("https://github.com/owner/repo"), github("owner/repo"));
        assert_eq!(page("https://github.com/owner/repo/"), github("owner/repo"));
    }

    #[test]
    fn ssh_url_remote_becomes_https_page_without_its_ssh_port() {
        assert_eq!(page("ssh://git@github.com/owner/repo.git"), github("owner/repo"));
        assert_eq!(page("ssh://git@github.com:22/owner/repo"), github("owner/repo"));
        assert_eq!(page("ssh://github.com/owner/repo.git"), github("owner/repo"));
    }

    #[test]
    fn https_remote_on_its_own_port_keeps_the_port() {
        assert_eq!(
            page("https://git.example.com:8443/team/app.git"),
            Some(("team/app".to_owned(), "https://git.example.com:8443/team/app".to_owned())),
        );
    }

    /// Azure DevOps SSH paths carry a `v3/` prefix and its pages a `_git` segment.
    #[test]
    fn azure_devops_ssh_remote_becomes_its_web_page() {
        let expected = Some(("org/project/repo".to_owned(), "https://dev.azure.com/org/project/_git/repo".to_owned()));
        assert_eq!(page("git@ssh.dev.azure.com:v3/org/project/repo"), expected);
        assert_eq!(page("ssh://git@ssh.dev.azure.com/v3/org/project/repo"), expected);
        assert_eq!(page("git@ssh.dev.azure.com:v3/org/repo"), None);
        assert_eq!(page("git@vs-ssh.visualstudio.com:v3/org/project/repo"), None);
    }

    #[test]
    fn azure_devops_https_remote_is_already_its_web_page() {
        assert_eq!(
            page("https://org@dev.azure.com/org/project/_git/repo"),
            Some(("org/project/_git/repo".to_owned(), "https://dev.azure.com/org/project/_git/repo".to_owned())),
        );
    }

    #[test]
    fn local_paths_and_other_schemes_have_no_page() {
        for remote in [
            "",
            r"C:\src\repo",
            "C:/src/repo.git",
            "/srv/git/repo.git",
            "../repo",
            r"\\server\share\repo",
            "file:///srv/git/repo.git",
            "git://github.com/owner/repo.git",
            "http://github.com/owner/repo.git",
            "ftp://github.com/owner/repo.git",
        ] {
            assert_eq!(page(remote), None, "{remote:?}");
        }
    }

    #[test]
    fn a_remote_without_owner_and_repo_has_no_page() {
        for remote in ["git@github.com:repo.git", "https://github.com/", "https://github.com", "git@github.com:"] {
            assert_eq!(page(remote), None, "{remote:?}");
        }
    }

    /// The URL reaches the system opener as an argument, so only plain
    /// host and path characters get through.
    #[test]
    fn a_remote_with_unexpected_characters_has_no_page() {
        for remote in [
            "https://github.com/owner/re po",
            "https://github.com/owner/repo\"&calc",
            "git@github.com:owner/../repo",
            "https://git hub.com/owner/repo",
            "https://github.com/owner/repo?x=1",
            "https://github.com/owner//repo",
        ] {
            assert_eq!(page(remote), None, "{remote:?}");
        }
    }

    #[test]
    fn credentials_in_an_https_remote_are_dropped() {
        assert_eq!(page("https://user:ghp_secret@github.com/owner/repo.git"), github("owner/repo"));
        assert_eq!(page("https://ghp_secret@github.com/owner/repo"), github("owner/repo"));
    }
}
