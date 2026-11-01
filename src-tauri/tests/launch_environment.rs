//! The environment a launched Agent gets (spec §7.4): what npm and cargo added
//! to the app's own environment is taken out again.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use code_orchestrate_lib::launch_env::{Environment, PermanentEnvironment, launch_environment, search_path};

/// Holds these names in the user's permanent environment.
struct Permanent(&'static [&'static str]);

impl PermanentEnvironment for Permanent {
    fn has(&self, name: &OsStr) -> bool {
        self.0.iter().any(|n| OsStr::new(n).eq_ignore_ascii_case(name))
    }
}

const NOTHING_PERMANENT: Permanent = Permanent(&[]);

fn env(vars: &[(&str, &str)]) -> Environment {
    vars.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))).collect()
}

fn names(environment: &Environment) -> Vec<String> {
    environment.iter().map(|(k, _)| k.to_string_lossy().into_owned()).collect()
}

/// Started by npm, on top of `vars`.
fn from_npm(vars: &[(&str, &str)]) -> Environment {
    let mut all = env(&[("npm_lifecycle_event", "tauri")]);
    all.extend(env(vars));
    all
}

#[test]
fn without_npm_the_environment_is_returned_unchanged() {
    let inherited = env(&[
        ("CARGO_PKG_NAME", "x"),
        ("HOME", "h"),
        ("PATH", r"E:\app\node_modules\.bin;C:\Windows"),
        ("npm_config_cache", "c"),
    ]);
    assert_eq!(launch_environment(inherited.clone(), &NOTHING_PERMANENT), inherited);
}

#[test]
fn the_variables_the_dev_toolchain_sets_are_removed() {
    let inherited = from_npm(&[
        ("npm_command", "run"),
        ("npm_config_local_prefix", r"E:\app"),
        ("INIT_CWD", r"E:\app"),
        ("NODE", "node.exe"),
        ("HOME", "h"),
        ("EDITOR", "notepad"),
        ("COLOR", "0"),
        ("PROMPT", "$P$G"),
        ("CARGO", "cargo.exe"),
        ("CARGO_PKG_NAME", "code-orchestrate"),
        ("CARGO_TERM_PROGRESS_WHEN", "always"),
        ("OUT_DIR", "out"),
        ("LD_LIBRARY_PATH", "lib"),
        ("RUST_RECURSION_COUNT", "1"),
        ("RUSTUP_TOOLCHAIN", "stable"),
        ("RUSTUP_TOOLCHAIN_SOURCE", "default"),
        ("TAURI_ENV_TARGET_TRIPLE", "x86_64"),
        ("TAURI_CLI_VERBOSITY", "0"),
    ]);
    assert_eq!(names(&launch_environment(inherited, &NOTHING_PERMANENT)), Vec::<String>::new());
}

#[test]
fn everything_else_is_left_alone() {
    let kept = [("USERPROFILE", r"C:\Users\me"), ("HTTPS_PROXY", "http://proxy"), ("MY_OWN", "1"), ("CARGOES", "2")];
    let cleaned = launch_environment(from_npm(&kept), &NOTHING_PERMANENT);
    assert_eq!(names(&cleaned), ["USERPROFILE", "HTTPS_PROXY", "MY_OWN", "CARGOES"]);
}

#[test]
fn a_toolchain_named_variable_in_the_permanent_environment_is_kept() {
    let inherited = from_npm(&[("CARGO_TARGET_DIR", r"D:\t"), ("npm_config_cache", r"D:\npm"), ("HOME", "h"), ("EDITOR", "e")]);
    let cleaned = launch_environment(inherited, &Permanent(&["CARGO_TARGET_DIR", "NPM_CONFIG_CACHE", "home"]));
    assert_eq!(names(&cleaned), ["CARGO_TARGET_DIR", "npm_config_cache", "HOME"]);
}

#[test]
fn cargo_home_and_rustup_home_are_always_kept() {
    let inherited = from_npm(&[("CARGO_HOME", "c"), ("RUSTUP_HOME", "r")]);
    assert_eq!(names(&launch_environment(inherited, &NOTHING_PERMANENT)), ["CARGO_HOME", "RUSTUP_HOME"]);
}

#[cfg(windows)]
#[test]
fn variable_names_match_case_insensitively_on_windows() {
    let inherited = env(&[("NPM_LIFECYCLE_EVENT", "tauri"), ("Npm_Command", "run"), ("cargo_pkg_name", "x"), ("Tauri_Env_X", "y")]);
    assert_eq!(names(&launch_environment(inherited, &NOTHING_PERMANENT)), Vec::<String>::new());
}

fn path_of(environment: &Environment) -> Vec<String> {
    search_path(environment).iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

/// The `PATH` left over from what `tauri dev` put together on a real machine.
#[cfg(windows)]
#[test]
fn the_dev_toolchains_path_entries_are_removed_and_the_rest_keep_their_order() {
    let path = [
        r"E:\work\app\src-tauri\target\debug\build\libsqlite3-sys-ac06\out",
        r"E:\work\app\src-tauri\target\debug",
        r"E:\work\app\src-tauri\target\debug\deps",
        r"C:\Users\me\.rustup\toolchains\stable-x86_64-pc-windows-msvc\lib\rustlib\x86_64-pc-windows-msvc\lib",
        r"E:\work\app\node_modules\.bin",
        r"E:\work\node_modules\.bin",
        r"E:\node_modules\.bin",
        r"C:\nvm\v23\node_modules\npm\node_modules\@npmcli\run-script\lib\node-gyp-bin",
        r"C:\Users\me\bin",
        r"C:\Windows\system32",
        r"C:\Users\me\.cargo\bin",
    ]
    .join(";");
    let inherited = from_npm(&[
        ("npm_config_local_prefix", r"E:\work\app"),
        ("CARGO_MANIFEST_DIR", r"E:\work\app\src-tauri"),
        ("Path", &path),
    ]);
    let cleaned = launch_environment(inherited, &NOTHING_PERMANENT);
    assert_eq!(path_of(&cleaned), [r"C:\Users\me\bin", r"C:\Windows\system32", r"C:\Users\me\.cargo\bin"]);
    assert_eq!(names(&cleaned), ["Path"]);
}

/// Another project's tools, or a folder merely named `target`, are the user's.
#[cfg(windows)]
#[test]
fn path_entries_that_only_look_like_the_toolchains_are_kept() {
    let kept = [r"E:\other\node_modules\.bin", r"C:\tools\lib\rustlib\x86\bin", r"E:\work\app\target-tools", r"C:\target\debug", r"E:\work\app\bin"];
    let inherited = from_npm(&[("INIT_CWD", r"E:\work\app"), ("PATH", &kept.join(";"))]);
    assert_eq!(path_of(&launch_environment(inherited, &NOTHING_PERMANENT)), kept);
}

#[cfg(windows)]
#[test]
fn path_entries_match_case_insensitively_and_across_separators_on_windows() {
    let inherited = from_npm(&[
        ("npm_config_local_prefix", r"e:\WORK\App"),
        ("PATH", r"E:/work/app/NODE_MODULES/.bin/;E:\Work\APP\Src-Tauri\Target\Debug;C:\Windows"),
    ]);
    assert_eq!(path_of(&launch_environment(inherited, &NOTHING_PERMANENT)), [r"C:\Windows"]);
}

#[cfg(windows)]
#[test]
fn a_custom_cargo_target_folder_counts_as_the_toolchains() {
    let inherited = from_npm(&[("CARGO_TARGET_DIR", r"D:\build"), ("PATH", r"D:\build\debug;C:\Windows")]);
    assert_eq!(path_of(&launch_environment(inherited, &NOTHING_PERMANENT)), [r"C:\Windows"]);
}

#[cfg(windows)]
#[test]
fn a_relative_cargo_target_folder_is_found_under_the_repo() {
    let inherited = from_npm(&[
        ("CARGO_MANIFEST_DIR", r"E:\work\app\src-tauri"),
        ("CARGO_TARGET_DIR", "build-output"),
        ("PATH", r"E:\work\app\src-tauri\build-output\debug;C:\Windows"),
    ]);
    assert_eq!(path_of(&launch_environment(inherited, &NOTHING_PERMANENT)), [r"C:\Windows"]);
}

#[cfg(windows)]
#[test]
fn a_cargo_target_folder_above_the_manifest_is_found() {
    let inherited = from_npm(&[
        ("CARGO_MANIFEST_DIR", r"E:\work\app\src-tauri"),
        ("CARGO_TARGET_DIR", r"..\build-cache"),
        ("PATH", r"E:\work\app\build-cache\debug;C:\Windows"),
    ]);
    assert_eq!(path_of(&launch_environment(inherited, &NOTHING_PERMANENT)), [r"C:\Windows"]);
}

#[test]
fn the_search_path_of_an_environment_without_path_is_empty() {
    assert_eq!(search_path(&env(&[("X", "1")])), Vec::<PathBuf>::new());
}

/// Settings and a launch both look `claude` up on this `PATH`.
#[cfg(windows)]
#[test]
fn claude_resolves_past_the_copy_in_the_apps_node_modules() {
    use code_orchestrate_lib::agents::AgentAdapter;
    use code_orchestrate_lib::agents::claude_code::ClaudeCode;

    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("app").join("node_modules").join(".bin");
    let global = dir.path().join("global");
    std::fs::create_dir_all(&local).unwrap();
    std::fs::create_dir_all(&global).unwrap();
    std::fs::write(local.join("claude.cmd"), "").unwrap();
    std::fs::write(global.join("claude.exe"), "").unwrap();

    let path = std::env::join_paths([&local, &global]).unwrap();
    let inherited = from_npm(&[
        ("npm_config_local_prefix", dir.path().join("app").to_str().unwrap()),
        ("PATH", path.to_str().unwrap()),
    ]);
    let dirs = search_path(&launch_environment(inherited, &NOTHING_PERMANENT));
    let resolved = ClaudeCode::with_search_path(dirs).resolve_executable();
    assert_eq!(resolved, Some(global.join("claude.exe").to_string_lossy().into_owned()));
}
