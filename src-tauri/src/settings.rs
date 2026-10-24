//! `settings.json`: the hand-editable settings file (spec §9.4).
//!
//! The file is read fresh on every load, so hand edits are picked up on
//! startup and window focus. Writes are sparse (only non-default keys), keep
//! unknown keys and key order, and are refused while the file is not valid
//! JSON, so a hand edit in progress is never clobbered.
//!
//! To add a setting, add one [`Def`] to [`DEFS`]. Keys are dotted paths
//! (`terminal.preset`) stored as nested objects in the file.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::launch::{CustomTemplate, LaunchError, Terminal};

/// The settings file's name inside the app data folder (spec §4.1).
pub const SETTINGS_FILE_NAME: &str = "settings.json";

/// One setting: its dotted key, default and validator.
struct Def {
    key: &'static str,
    default: fn() -> Value,
    /// `Err` holds a human-readable reason, shown as the inline warning.
    check: fn(&Value) -> Result<(), String>,
    /// The values a choice setting offers on this platform (empty: free-form).
    choices: &'static [&'static str],
}

/// Every setting the app knows (spec §9.1).
const DEFS: &[Def] = &[
    Def { key: "theme", default: || json!("system"), check: check_theme, choices: THEMES },
    Def { key: "terminal.preset", default: || json!(PRESETS[0]), check: check_preset, choices: PRESETS },
    Def {
        key: "terminal.windowsTerminal.target",
        default: || json!("newWindow"),
        check: check_wt_target,
        choices: WT_TARGETS,
    },
    Def { key: "terminal.custom", default: || Value::Null, check: check_custom, choices: &[] },
];

const THEMES: &[&str] = &["system", "light", "dark"];
/// Non-Windows platforms offer only Custom in v1 (spec §7.2). The first is the default.
const PRESETS: &[&str] = if cfg!(windows) { &["windowsTerminal", "console", "custom"] } else { &["custom"] };
const WT_TARGETS: &[&str] = &["newWindow", "newTab"];

fn check_theme(value: &Value) -> Result<(), String> {
    one_of(value, THEMES)
}

fn check_preset(value: &Value) -> Result<(), String> {
    one_of(value, PRESETS)
}

fn check_wt_target(value: &Value) -> Result<(), String> {
    one_of(value, WT_TARGETS)
}

/// `null` (none) or an argv array that [`CustomTemplate`] accepts.
fn check_custom(value: &Value) -> Result<(), String> {
    match value {
        Value::Null => Ok(()),
        _ => custom_template(value).map(drop),
    }
}

fn custom_template(value: &Value) -> Result<CustomTemplate, String> {
    let not_argv = || "The custom terminal must be an array of strings".to_string();
    let argv = value.as_array().ok_or_else(not_argv)?;
    let argv = argv
        .iter()
        .map(|a| a.as_str().map(str::to_owned).ok_or_else(not_argv))
        .collect::<Result<Vec<_>, _>>()?;
    CustomTemplate::new(argv)
}

/// Accept only one of the given strings.
fn one_of(value: &Value, allowed: &[&str]) -> Result<(), String> {
    match value.as_str() {
        Some(s) if allowed.contains(&s) => Ok(()),
        _ => Err(format!("{value} is not one of: {}", allowed.join(", "))),
    }
}

fn def(key: &str) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.key == key)
}

/// The app theme. `System` follows the OS (spec §8.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// The effective settings, as the UI sees them.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Every known setting by dotted key, with defaults filled in and invalid
    /// values replaced by their defaults.
    pub values: BTreeMap<String, Value>,
    /// Dotted key -> why the file's value was ignored.
    pub warnings: BTreeMap<String, String>,
    /// Why the whole file was ignored (invalid JSON, unreadable). While set,
    /// saves are refused until the file is fixed or reset.
    pub file_error: Option<String>,
    /// Choice setting's dotted key -> the values offered on this platform.
    pub choices: BTreeMap<String, Vec<Value>>,
}

impl Snapshot {
    /// The terminal Sessions open in (spec §7.2).
    pub fn terminal(&self) -> Result<Terminal, LaunchError> {
        let value = |key: &str| self.values.get(key).cloned().unwrap_or(Value::Null);
        match value("terminal.preset").as_str() {
            Some("console") => Ok(Terminal::ConsoleWindow),
            Some("custom") => match custom_template(&value("terminal.custom")) {
                Ok(template) => Ok(Terminal::Custom(template)),
                Err(_) => Err(LaunchError::TerminalNotConfigured {
                    message: "No custom terminal is set. Add a template in Settings › Terminal.".into(),
                }),
            },
            _ => Ok(match value("terminal.windowsTerminal.target").as_str() {
                Some("newTab") => Terminal::WindowsTerminalNewTab,
                _ => Terminal::WindowsTerminalNewWindow,
            }),
        }
    }

    pub fn theme(&self) -> Theme {
        match self.values.get("theme").and_then(Value::as_str) {
            Some("light") => Theme::Light,
            Some("dark") => Theme::Dark,
            _ => Theme::System,
        }
    }
}

/// A settings error for the UI: `{ kind, message }`.
#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SettingsError {
    #[error("Unknown setting: {key}")]
    UnknownKey { key: String, message: String },
    #[error("{message}")]
    InvalidValue { key: String, message: String },
    #[error("{message}")]
    FileInvalid { message: String },
    #[error("{message}")]
    Internal { message: String },
}

impl SettingsError {
    fn internal(e: impl ToString) -> Self {
        SettingsError::Internal { message: e.to_string() }
    }
}

/// The parsed file: its top-level object, or why it could not be used.
enum Raw {
    Object(Map<String, Value>),
    Invalid(String),
}

/// Handle on one `settings.json`.
#[derive(Debug, Clone)]
pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn new(path: PathBuf) -> Self {
        SettingsFile { path }
    }

    /// Read the file and work out the effective settings. Never fails: a
    /// missing file gives the defaults, a broken one sets `file_error`.
    pub fn load(&self) -> Snapshot {
        snapshot(&self.read())
    }

    /// Validate and save one setting, then return the new snapshot.
    pub fn set(&self, key: &str, value: Value) -> Result<Snapshot, SettingsError> {
        let Some(def) = def(key) else {
            return Err(SettingsError::UnknownKey {
                key: key.into(),
                message: format!("Unknown setting: {key}"),
            });
        };
        (def.check)(&value).map_err(|message| SettingsError::InvalidValue { key: key.into(), message })?;
        let mut root = match self.read() {
            Raw::Object(root) => root,
            Raw::Invalid(error) => {
                return Err(SettingsError::FileInvalid {
                    message: format!("settings.json is not valid, so it was not changed: {error}"),
                });
            }
        };
        if value == (def.default)() {
            remove_path(&mut root, key);
        } else {
            set_path(&mut root, key, value);
        }
        self.write(&root)?;
        Ok(snapshot(&Raw::Object(root)))
    }

    /// Replace the file with an empty one, whatever it holds.
    pub fn reset(&self) -> Result<Snapshot, SettingsError> {
        let root = Map::new();
        self.write(&root)?;
        Ok(snapshot(&Raw::Object(root)))
    }

    fn read(&self) -> Raw {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Raw::Object(Map::new()),
            Err(e) => return Raw::Invalid(format!("could not read the file: {e}")),
        };
        if text.trim().is_empty() {
            return Raw::Object(Map::new());
        }
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(root)) => Raw::Object(root),
            Ok(_) => Raw::Invalid("the top level must be a JSON object { ... }".into()),
            Err(e) => Raw::Invalid(e.to_string()),
        }
    }

    /// Write via a temporary file and rename, so a crash never leaves a
    /// half-written file.
    fn write(&self, root: &Map<String, Value>) -> Result<(), SettingsError> {
        let mut text = serde_json::to_string_pretty(root).map_err(SettingsError::internal)?;
        text.push('\n');
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(SettingsError::internal)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(SettingsError::internal)?;
        std::fs::rename(&tmp, &self.path).map_err(SettingsError::internal)
    }
}

fn snapshot(raw: &Raw) -> Snapshot {
    let (root, file_error) = match raw {
        Raw::Object(root) => (Some(root), None),
        Raw::Invalid(error) => (None, Some(error.clone())),
    };
    let mut values = BTreeMap::new();
    let mut warnings = BTreeMap::new();
    for def in DEFS {
        let value = match root.and_then(|r| get_path(r, def.key)) {
            None => (def.default)(),
            Some(found) => match (def.check)(found) {
                Ok(()) => found.clone(),
                Err(reason) => {
                    warnings.insert(def.key.to_string(), format!("Using the default: {reason}"));
                    (def.default)()
                }
            },
        };
        values.insert(def.key.to_string(), value);
    }
    let choices = DEFS
        .iter()
        .filter(|d| !d.choices.is_empty())
        .map(|d| (d.key.to_string(), d.choices.iter().map(|c| json!(c)).collect()))
        .collect();
    Snapshot { values, warnings, file_error, choices }
}

fn get_path<'a>(root: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    let (parents, leaf) = split(key);
    let mut map = root;
    for part in parents {
        map = map.get(part)?.as_object()?;
    }
    map.get(leaf)
}

/// Set a dotted key, creating (or replacing non-object) parents as needed.
fn set_path(root: &mut Map<String, Value>, key: &str, value: Value) {
    let (parents, leaf) = split(key);
    let mut map = root;
    for part in parents {
        let slot = map.entry(part.to_string()).or_insert_with(|| Value::Object(Map::new()));
        if !slot.is_object() {
            *slot = Value::Object(Map::new());
        }
        map = slot.as_object_mut().expect("just made an object");
    }
    map.insert(leaf.to_string(), value);
}

/// Remove a dotted key, then prune parents it left empty.
fn remove_path(root: &mut Map<String, Value>, key: &str) {
    fn go(map: &mut Map<String, Value>, parts: &[&str]) {
        match parts {
            [] => {}
            [leaf] => {
                map.shift_remove(*leaf);
            }
            [head, rest @ ..] => {
                if let Some(Value::Object(child)) = map.get_mut(*head) {
                    go(child, rest);
                    if child.is_empty() {
                        map.shift_remove(*head);
                    }
                }
            }
        }
    }
    let parts: Vec<&str> = key.split('.').collect();
    go(root, &parts);
}

fn split(key: &str) -> (Vec<&str>, &str) {
    let mut parts: Vec<&str> = key.split('.').collect();
    let leaf = parts.pop().unwrap_or(key);
    (parts, leaf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn dotted_keys_are_nested_objects() {
        let mut root = obj(json!({ "terminal": { "keep": 1 } }));
        set_path(&mut root, "terminal.windowsTerminal.target", json!("tab"));
        assert_eq!(
            Value::Object(root.clone()),
            json!({ "terminal": { "keep": 1, "windowsTerminal": { "target": "tab" } } })
        );
        assert_eq!(get_path(&root, "terminal.windowsTerminal.target"), Some(&json!("tab")));
        assert_eq!(get_path(&root, "terminal.keep.deeper"), None);
    }

    #[test]
    fn removing_a_dotted_key_prunes_only_empty_parents() {
        let mut root = obj(json!({ "a": { "b": { "c": 1 } }, "t": { "x": 1, "y": 2 } }));
        remove_path(&mut root, "a.b.c");
        remove_path(&mut root, "t.x");
        assert_eq!(Value::Object(root), json!({ "t": { "y": 2 } }));
    }

    #[test]
    fn every_default_passes_its_own_check() {
        for def in DEFS {
            assert_eq!((def.check)(&(def.default)()), Ok(()), "{}", def.key);
        }
    }
}
