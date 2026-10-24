//! Wrapping an Agent's command in a terminal (spec §7.2).

use code_orchestrate_lib::agents::LaunchCommand;
use code_orchestrate_lib::launch::{CustomTemplate, Terminal, TerminalCommand};

fn claude(cwd: &str, args: &[&str]) -> LaunchCommand {
    LaunchCommand {
        program: r"C:\Users\me\.local\bin\claude.exe".into(),
        args: args.iter().map(|a| a.to_string()).collect(),
        cwd: cwd.into(),
    }
}

fn wt(args: &[&str]) -> TerminalCommand {
    TerminalCommand { program: "wt.exe".into(), args: args.iter().map(|a| a.to_string()).collect(), current_dir: None }
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

#[test]
fn windows_terminal_can_open_a_new_tab_in_the_most_recent_window() {
    let wrapped = Terminal::WindowsTerminalNewTab.wrap(&claude(r"E:\code\app", &["--continue"]));

    assert_eq!(
        wrapped,
        wt(&["-w", "0", "new-tab", "-d", r"E:\code\app", r"C:\Users\me\.local\bin\claude.exe", "--continue"])
    );
}

fn console(args: &[&str]) -> TerminalCommand {
    TerminalCommand { program: "cmd.exe".into(), args: args.iter().map(|a| a.to_string()).collect(), current_dir: None }
}

#[test]
fn a_console_window_starts_the_command_with_an_empty_title() {
    let wrapped = Terminal::ConsoleWindow.wrap(&claude(r"C:\My Projects\app", &["--continue"]));

    assert_eq!(
        wrapped,
        console(&["/c", "start", "", "/D", r"C:\My Projects\app", r"C:\Users\me\.local\bin\claude.exe", "--continue"])
    );
}

#[test]
fn a_console_window_caret_escapes_cmd_metacharacters_in_unquoted_arguments() {
    // `cmd /c` reparses its command line, and an argument without whitespace
    // reaches it unquoted. A shim path already escaped for its own `cmd /k`
    // is escaped again, so each cmd strips one level.
    let cmd = LaunchCommand {
        program: "cmd".into(),
        args: vec!["/k".into(), r"C:\Users\a^&b\claude.cmd".into(), "--resume".into(), "x".into()],
        cwd: r"E:\r&d".into(),
    };

    let wrapped = Terminal::ConsoleWindow.wrap(&cmd);

    assert_eq!(
        wrapped,
        console(&["/c", "start", "", "/D", r"E:\r^&d", "cmd", "/k", r"C:\Users\a^^^&b\claude.cmd", "--resume", "x"])
    );
}

fn template(argv: &[&str]) -> Result<CustomTemplate, String> {
    CustomTemplate::new(argv.iter().map(|a| a.to_string()).collect())
}

#[test]
fn a_custom_template_puts_cwd_in_one_argument_and_spreads_the_command() {
    let custom = template(&["alacritty", "--working-directory", "{cwd}", "-e", "{command}"]).unwrap();

    let wrapped = Terminal::Custom(custom).wrap(&claude(r"C:\My Projects\app", &["--resume", "abc"]));

    assert_eq!(
        wrapped,
        TerminalCommand {
            program: "alacritty".into(),
            args: vec![
                "--working-directory".into(),
                r"C:\My Projects\app".into(),
                "-e".into(),
                r"C:\Users\me\.local\bin\claude.exe".into(),
                "--resume".into(),
                "abc".into(),
            ],
            current_dir: Some(r"C:\My Projects\app".into()),
        }
    );
}

#[test]
fn a_custom_template_can_embed_cwd_in_an_argument() {
    let custom = template(&["kitty", "--directory={cwd}", "{command}"]).unwrap();

    let wrapped = Terminal::Custom(custom).wrap(&claude("/home/me/app", &[]));

    assert_eq!(wrapped.program, "kitty");
    assert_eq!(wrapped.args, vec!["--directory=/home/me/app".to_string(), r"C:\Users\me\.local\bin\claude.exe".into()]);
}

#[test]
fn a_custom_template_may_start_with_the_command() {
    let custom = template(&["{command}"]).unwrap();

    let wrapped = Terminal::Custom(custom).wrap(&claude(r"E:\w", &["--continue"]));

    assert_eq!(
        wrapped,
        TerminalCommand { program: r"C:\Users\me\.local\bin\claude.exe".into(), args: vec!["--continue".into()], current_dir: Some(r"E:\w".into()) }
    );
}

#[test]
fn a_custom_template_without_command_is_rejected() {
    assert!(template(&["alacritty", "--working-directory", "{cwd}"]).is_err());
    assert!(template(&[]).is_err());
}

#[test]
fn a_custom_template_needs_command_as_a_whole_argument() {
    assert!(template(&["xterm", "-e={command}"]).is_err());
}

#[test]
fn a_console_window_keeps_a_percent_in_the_workspace_path_away_from_cmd() {
    // cmd expands `%NAME%` even inside quotes, so a path with `%` is never put
    // on cmd's command line: cmd starts in that directory and `start` uses it.
    let wrapped = Terminal::ConsoleWindow.wrap(&claude(r"C:\repos\%USERNAME% stuff", &[]));

    assert_eq!(
        wrapped,
        TerminalCommand {
            current_dir: Some(r"C:\repos\%USERNAME% stuff".into()),
            ..console(&["/c", "start", "", "/D", ".", r"C:\Users\me\.local\bin\claude.exe"])
        }
    );
}

#[test]
fn a_console_window_leaves_metacharacters_alone_inside_a_quoted_argument() {
    let wrapped = Terminal::ConsoleWindow.wrap(&claude(r"E:\R & D", &[]));

    assert_eq!(wrapped.args[4], r"E:\R & D");
}
