//! The environment a launched Agent gets (spec §7.4). When npm started the
//! app, its environment holds what npm and cargo set up for building it, which
//! a Session must not see. The app's own git and `--version` probe don't use
//! this.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// A process environment, as name and value pairs.
pub type Environment = Vec<(OsString, OsString)>;

/// The variables set permanently for the user, as opposed to for this process.
pub trait PermanentEnvironment {
    fn has(&self, name: &OsStr) -> bool;
}

/// Set by npm for the scripts it runs, so it means npm started the app.
const STARTED_BY_NPM: &str = "npm_lifecycle_event";
/// The toolchain's variables, as whole names.
const TOOLCHAIN_NAMES: [&str; 12] = [
    "INIT_CWD",
    "NODE",
    "HOME",
    "EDITOR",
    "COLOR",
    "PROMPT",
    "CARGO",
    "OUT_DIR",
    "LD_LIBRARY_PATH",
    "RUST_RECURSION_COUNT",
    "RUSTUP_TOOLCHAIN",
    "RUSTUP_TOOLCHAIN_SOURCE",
];
/// The toolchain's variables, by prefix.
const TOOLCHAIN_PREFIXES: [&str; 3] = ["npm_", "CARGO_", "TAURI_"];
/// Toolchain-named, but they say where the user's installs live.
const ALWAYS_KEPT: [&str; 2] = ["CARGO_HOME", "RUSTUP_HOME"];

/// The environment for a launch: `inherited` without what the dev toolchain
/// added, or unchanged when npm did not start the app. Variables the user set
/// permanently stay, and so does everything that isn't the toolchain's.
pub fn launch_environment(inherited: Environment, permanent: &dyn PermanentEnvironment) -> Environment {
    if lookup(&inherited, STARTED_BY_NPM).is_none() {
        return inherited;
    }
    let build_folders = BuildFolders::of(&inherited);
    inherited
        .iter()
        .filter(|(name, _)| !is_toolchain_name(name) || is_kept(name) || permanent.has(name))
        .map(|(name, value)| {
            let value = if same_name(name, "PATH") { build_folders.clean(value) } else { value.clone() };
            (name.clone(), value)
        })
        .collect()
}

/// The environment for a launch from this process, read afresh.
pub fn current() -> Environment {
    launch_environment(std::env::vars_os().collect(), &OsPermanent)
}

/// The user and system variables in the registry, as a new login would get
/// them. Only names are read, and a variable set in either place counts.
#[cfg(windows)]
struct OsPermanent;

#[cfg(windows)]
impl PermanentEnvironment for OsPermanent {
    fn has(&self, name: &OsStr) -> bool {
        use std::os::windows::ffi::OsStrExt;
        use std::ptr::null_mut;
        use windows_sys::Win32::Foundation::ERROR_SUCCESS;
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_ANY, RegGetValueW};
        use windows_sys::w;

        let name: Vec<u16> = name.encode_wide().chain([0]).collect();
        [
            (HKEY_CURRENT_USER, w!("Environment")),
            (HKEY_LOCAL_MACHINE, w!(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")),
        ]
        .into_iter()
        .any(|(root, key)| {
            // With nowhere to put the value, this only says whether it exists.
            let status =
                unsafe { RegGetValueW(root, key, name.as_ptr(), RRF_RT_ANY, null_mut(), null_mut(), null_mut()) };
            status == ERROR_SUCCESS
        })
    }
}

/// No permanent environment is read outside Windows. `HOME` and `EDITOR` come
/// from the user's login or shell there, and a Session needs `HOME`.
#[cfg(not(windows))]
struct OsPermanent;

#[cfg(not(windows))]
impl PermanentEnvironment for OsPermanent {
    fn has(&self, name: &OsStr) -> bool {
        name == "HOME" || name == "EDITOR"
    }
}

/// The folders on the environment's `PATH`, in order.
pub fn search_path(environment: &[(OsString, OsString)]) -> Vec<PathBuf> {
    lookup(environment, "PATH").map(|path| std::env::split_paths(path).collect()).unwrap_or_default()
}

fn is_toolchain_name(name: &OsStr) -> bool {
    let name = fold_name(name);
    TOOLCHAIN_NAMES.iter().any(|n| name == fold_name(OsStr::new(n)))
        || TOOLCHAIN_PREFIXES.iter().any(|p| name.starts_with(&fold_name(OsStr::new(p))))
}

fn is_kept(name: &OsStr) -> bool {
    ALWAYS_KEPT.iter().any(|n| same_name(name, n))
}

fn lookup<'a>(environment: &'a [(OsString, OsString)], name: &str) -> Option<&'a OsStr> {
    environment.iter().find(|(n, _)| same_name(n, name)).map(|(_, value)| value.as_os_str())
}

fn same_name(a: &OsStr, b: &str) -> bool {
    fold_name(a) == fold_name(OsStr::new(b))
}

/// Windows variable names ignore case.
fn fold_name(name: &OsStr) -> String {
    let name = name.to_string_lossy();
    if cfg!(windows) { name.to_uppercase() } else { name.into_owned() }
}

/// Windows paths ignore case and take either separator.
fn fold_path(path: &Path) -> PathBuf {
    let path = path.to_string_lossy();
    PathBuf::from(if cfg!(windows) { path.to_lowercase().replace('/', "\\") } else { path.into_owned() })
}

/// Where the toolchain's `PATH` entries point, read from its own variables.
struct BuildFolders {
    /// The app's repo: `node_modules\.bin` here or above is npm's.
    roots: Vec<PathBuf>,
    /// cargo's output folders, whose subfolders it also adds.
    targets: Vec<PathBuf>,
}

impl BuildFolders {
    fn of(environment: &[(OsString, OsString)]) -> Self {
        let roots: Vec<PathBuf> = ["npm_config_local_prefix", "INIT_CWD", "CARGO_MANIFEST_DIR"]
            .iter()
            .filter_map(|name| lookup(environment, name))
            .map(|root| fold_path(Path::new(root)))
            .collect();
        let mut targets: Vec<PathBuf> =
            roots.iter().flat_map(|root| [root.join("target"), root.join("src-tauri").join("target")]).collect();
        if let Some(dir) = lookup(environment, "CARGO_TARGET_DIR").map(|dir| fold_path(Path::new(dir))) {
            // A relative one is relative to where cargo ran; an absolute one replaces the root.
            let joined = roots.iter().map(|root| root.join(&dir));
            targets.extend(joined.map(|path| std::path::absolute(&path).unwrap_or(path)));
            targets.push(dir);
        }
        BuildFolders { roots, targets }
    }

    fn clean(&self, path: &OsStr) -> OsString {
        let entries = std::env::split_paths(path).filter(|entry| !self.added_by_toolchain(entry));
        // Entries split from a `PATH` can always be joined into one.
        std::env::join_paths(entries).unwrap_or_else(|_| path.to_owned())
    }

    fn added_by_toolchain(&self, entry: &Path) -> bool {
        let entry = fold_path(entry);
        self.targets.iter().any(|target| entry.starts_with(target))
            || self.is_npm_bin(&entry)
            || entry.ends_with("node-gyp-bin")
            || is_sysroot_library(&entry)
    }

    /// `<folder>\node_modules\.bin`, where `<folder>` is the repo or above it.
    fn is_npm_bin(&self, entry: &Path) -> bool {
        entry.ends_with(Path::new("node_modules").join(".bin"))
            && grandparent(entry).is_some_and(|folder| self.roots.iter().any(|root| root.starts_with(folder)))
    }
}

/// `…\lib\rustlib\<target>\lib`, which cargo adds for the program it runs.
fn is_sysroot_library(entry: &Path) -> bool {
    entry.ends_with("lib") && grandparent(entry).is_some_and(|folder| folder.ends_with(Path::new("lib").join("rustlib")))
}

fn grandparent(path: &Path) -> Option<&Path> {
    path.parent()?.parent()
}
