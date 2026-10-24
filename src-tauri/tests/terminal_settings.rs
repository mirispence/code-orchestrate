//! The Terminal settings (spec §9.1, §9.2) and the terminal they pick (§7.2).

use code_orchestrate_lib::launch::{CustomTemplate, LaunchError, Terminal};
use code_orchestrate_lib::settings::{SettingsError, SettingsFile};
use serde_json::{Value, json};

struct Fixture {
    dir: tempfile::TempDir,
}

fn fixture() -> Fixture {
    Fixture { dir: tempfile::tempdir().unwrap() }
}

impl Fixture {
    fn file(&self) -> SettingsFile {
        SettingsFile::new(self.dir.path().join("settings.json"))
    }
    fn write(&self, value: Value) {
        std::fs::write(self.dir.path().join("settings.json"), value.to_string()).unwrap();
    }
    fn json(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.dir.path().join("settings.json")).unwrap()).unwrap()
    }
}

fn custom(argv: &[&str]) -> Terminal {
    Terminal::Custom(CustomTemplate::new(argv.iter().map(|a| a.to_string()).collect()).unwrap())
}

#[cfg(windows)]
#[test]
fn on_windows_the_default_is_windows_terminal_in_a_new_window() {
    let snap = fixture().file().load();
    assert_eq!(snap.values["terminal.preset"], json!("windowsTerminal"));
    assert_eq!(snap.values["terminal.windowsTerminal.target"], json!("newWindow"));
    assert_eq!(snap.values["terminal.custom"], Value::Null);
    assert_eq!(snap.terminal(), Ok(Terminal::WindowsTerminalNewWindow));
}

#[cfg(windows)]
#[test]
fn on_windows_every_preset_is_offered() {
    let snap = fixture().file().load();
    assert_eq!(snap.choices["terminal.preset"], vec![json!("windowsTerminal"), json!("console"), json!("custom")]);
}

#[cfg(not(windows))]
#[test]
fn elsewhere_only_custom_is_offered_and_is_the_default() {
    let f = fixture();
    let snap = f.file().load();
    assert_eq!(snap.values["terminal.preset"], json!("custom"));
    assert_eq!(snap.choices["terminal.preset"], vec![json!("custom")]);
    assert!(f.file().set("terminal.preset", json!("console")).is_err());
}

#[cfg(windows)]
#[test]
fn the_new_tab_target_picks_the_most_recent_window() {
    let f = fixture();
    let snap = f.file().set("terminal.windowsTerminal.target", json!("newTab")).unwrap();
    assert_eq!(snap.terminal(), Ok(Terminal::WindowsTerminalNewTab));
    assert_eq!(f.json(), json!({ "terminal": { "windowsTerminal": { "target": "newTab" } } }));
}

#[cfg(windows)]
#[test]
fn the_console_preset_picks_a_console_window() {
    let f = fixture();
    let snap = f.file().set("terminal.preset", json!("console")).unwrap();
    assert_eq!(snap.terminal(), Ok(Terminal::ConsoleWindow));
}

#[test]
fn the_custom_template_is_stored_as_an_argv_array_and_used() {
    let f = fixture();
    let argv = json!(["alacritty", "--working-directory", "{cwd}", "-e", "{command}"]);
    f.file().set("terminal.custom", argv.clone()).unwrap();
    let snap = f.file().set("terminal.preset", json!("custom")).unwrap();
    assert_eq!(f.json()["terminal"]["custom"], argv);
    assert_eq!(snap.terminal(), Ok(custom(&["alacritty", "--working-directory", "{cwd}", "-e", "{command}"])));
}

#[test]
fn a_template_without_command_is_rejected() {
    let f = fixture();
    let refused = f.file().set("terminal.custom", json!(["alacritty", "{cwd}"])).unwrap_err();
    match refused {
        SettingsError::InvalidValue { message, .. } => assert!(message.contains("{command}"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_template_that_is_not_an_array_of_strings_is_rejected() {
    let f = fixture();
    assert!(f.file().set("terminal.custom", json!("alacritty -e {command}")).is_err());
    assert!(f.file().set("terminal.custom", json!(["alacritty", 1, "{command}"])).is_err());
}

#[test]
fn clearing_the_template_removes_it() {
    let f = fixture();
    f.write(json!({ "terminal": { "custom": ["{command}"] } }));
    f.file().set("terminal.custom", Value::Null).unwrap();
    assert_eq!(f.json(), json!({}));
}

#[test]
fn the_custom_preset_without_a_template_cannot_launch() {
    let f = fixture();
    f.write(json!({ "terminal": { "preset": "custom" } }));
    let err = f.file().load().terminal().unwrap_err();
    assert!(matches!(err, LaunchError::TerminalNotConfigured { .. }), "{err:?}");
    assert_eq!(serde_json::to_value(&err).unwrap()["kind"], json!("terminalNotConfigured"));
}

#[test]
fn a_hand_edited_template_without_command_falls_back_with_a_warning() {
    let f = fixture();
    f.write(json!({ "terminal": { "custom": ["xterm"] } }));
    let snap = f.file().load();
    assert_eq!(snap.values["terminal.custom"], Value::Null);
    assert!(snap.warnings["terminal.custom"].contains("{command}"));
}
