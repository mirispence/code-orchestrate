//! Wrapping an Agent's command in a terminal (spec §7.2).

use code_orchestrate_lib::agents::LaunchCommand;
use code_orchestrate_lib::launch::{Terminal, TerminalCommand};

fn claude(cwd: &str, args: &[&str]) -> LaunchCommand {
    LaunchCommand {
        program: r"C:\Users\me\.local\bin\claude.exe".into(),
        args: args.iter().map(|a| a.to_string()).collect(),
        cwd: cwd.into(),
    }
}

fn wt(args: &[&str]) -> TerminalCommand {
    TerminalCommand { program: "wt.exe".into(), args: args.iter().map(|a| a.to_string()).collect() }
}

#[test]
fn windows_terminal_opens_a_new_window_in_the_workspace() {
    let wrapped = Terminal::WindowsTerminalNewWindow.wrap(&claude(r"E:\code\app", &["--continue"]));

    assert_eq!(
        wrapped,
        wt(&["-w", "new", "new-tab", "-d", r"E:\code\app", r"C:\Users\me\.local\bin\claude.exe", "--continue"])
    );
}

#[test]
fn a_path_with_spaces_stays_one_argument() {
    let wrapped = Terminal::WindowsTerminalNewWindow.wrap(&claude(r"C:\My Projects\app", &[]));

    assert_eq!(
        wrapped,
        wt(&["-w", "new", "new-tab", "-d", r"C:\My Projects\app", r"C:\Users\me\.local\bin\claude.exe"])
    );
}

#[test]
fn a_semicolon_in_the_path_is_escaped_for_windows_terminal() {
    let wrapped = Terminal::WindowsTerminalNewWindow.wrap(&claude(r"C:\a;b", &[]));

    assert_eq!(
        wrapped,
        wt(&["-w", "new", "new-tab", "-d", r"C:\a\;b", r"C:\Users\me\.local\bin\claude.exe"])
    );
}

#[test]
fn a_semicolon_in_any_agent_argument_is_escaped() {
    let cmd = LaunchCommand {
        program: "cmd".into(),
        args: vec!["/k".into(), r"C:\npm;x\claude.cmd".into(), "--resume".into(), "a;b".into()],
        cwd: r"E:\w".into(),
    };

    let wrapped = Terminal::WindowsTerminalNewWindow.wrap(&cmd);

    assert_eq!(
        wrapped,
        wt(&["-w", "new", "new-tab", "-d", r"E:\w", "cmd", "/k", r"C:\npm\;x\claude.cmd", "--resume", r"a\;b"])
    );
}

#[test]
fn a_drive_root_keeps_its_backslash() {
    let wrapped = Terminal::WindowsTerminalNewWindow.wrap(&claude(r"C:\", &[]));

    assert_eq!(
        wrapped,
        wt(&["-w", "new", "new-tab", "-d", r"C:\", r"C:\Users\me\.local\bin\claude.exe"])
    );
}
