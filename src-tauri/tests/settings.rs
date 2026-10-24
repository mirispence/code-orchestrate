//! settings.json handling (spec §9.4).

use std::path::PathBuf;

use code_orchestrate_lib::settings::{SettingsError, SettingsFile, Theme};
use serde_json::{Value, json};

struct Fixture {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    Fixture { _dir: dir, path }
}

impl Fixture {
    fn file(&self) -> SettingsFile {
        SettingsFile::new(self.path.clone())
    }
    fn write(&self, text: &str) {
        std::fs::write(&self.path, text).unwrap();
    }
    fn read(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap()
    }
    fn json(&self) -> Value {
        serde_json::from_str(&self.read()).unwrap()
    }
}

#[test]
fn a_missing_file_gives_the_defaults() {
    let f = fixture();
    let snap = f.file().load();
    assert_eq!(snap.values["theme"], json!("system"));
    assert_eq!(snap.theme(), Theme::System);
    assert!(snap.warnings.is_empty());
    assert_eq!(snap.file_error, None);
    assert!(!f.path.exists(), "loading must not create the file");
}

#[test]
fn values_in_the_file_are_used() {
    let f = fixture();
    f.write(r#"{ "theme": "dark" }"#);
    let snap = f.file().load();
    assert_eq!(snap.values["theme"], json!("dark"));
    assert_eq!(snap.theme(), Theme::Dark);
}

#[test]
fn setting_a_value_saves_it_and_returns_the_new_snapshot() {
    let f = fixture();
    let snap = f.file().set("theme", json!("light")).unwrap();
    assert_eq!(snap.theme(), Theme::Light);
    assert_eq!(f.json(), json!({ "theme": "light" }));
    assert_eq!(f.file().load().theme(), Theme::Light);
}

#[test]
fn writes_are_sparse_so_a_default_value_removes_its_key() {
    let f = fixture();
    f.write(r#"{ "theme": "dark" }"#);
    f.file().set("theme", json!("system")).unwrap();
    assert_eq!(f.json(), json!({}));
}

#[test]
fn unknown_keys_survive_a_save_in_their_original_order() {
    let f = fixture();
    f.write(r#"{ "zeta": 1, "theme": "dark", "alpha": { "nested": [true] } }"#);
    f.file().set("theme", json!("light")).unwrap();
    assert_eq!(f.json(), json!({ "zeta": 1, "theme": "light", "alpha": { "nested": [true] } }));
    let text = f.read();
    assert!(text.find("zeta").unwrap() < text.find("alpha").unwrap(), "order kept: {text}");
}

#[test]
fn invalid_json_uses_defaults_reports_the_error_and_is_never_overwritten() {
    let f = fixture();
    let broken = "{ \"theme\": \"dark\", }";
    f.write(broken);

    let snap = f.file().load();
    assert_eq!(snap.theme(), Theme::System);
    let error = snap.file_error.expect("a parse error");
    assert!(error.contains("line 1"), "error names the position: {error}");

    let refused = f.file().set("theme", json!("light")).unwrap_err();
    assert!(matches!(refused, SettingsError::FileInvalid { .. }), "{refused:?}");
    assert_eq!(f.read(), broken);
}

#[test]
fn a_json_root_that_is_not_an_object_is_a_file_error() {
    let f = fixture();
    f.write("[1, 2]");
    let snap = f.file().load();
    assert!(snap.file_error.is_some());
    assert_eq!(snap.theme(), Theme::System);
}

#[test]
fn an_empty_file_counts_as_no_settings() {
    let f = fixture();
    f.write("  \n");
    let snap = f.file().load();
    assert_eq!(snap.file_error, None);
    assert_eq!(snap.theme(), Theme::System);
}

#[test]
fn reset_to_defaults_replaces_even_an_invalid_file() {
    let f = fixture();
    f.write("not json");
    let snap = f.file().reset().unwrap();
    assert_eq!(snap.file_error, None);
    assert_eq!(snap.theme(), Theme::System);
    assert_eq!(f.json(), json!({}));
}

#[test]
fn an_invalid_value_falls_back_to_its_default_with_a_warning() {
    let f = fixture();
    f.write(r#"{ "theme": "purple" }"#);
    let snap = f.file().load();
    assert_eq!(snap.values["theme"], json!("system"));
    assert!(snap.warnings["theme"].contains("purple"), "{:?}", snap.warnings);
    assert_eq!(snap.file_error, None);
}

#[test]
fn an_invalid_value_in_the_file_is_kept_when_another_change_is_saved() {
    let f = fixture();
    f.write(r#"{ "theme": "purple", "other": 1 }"#);
    f.file().set("theme", json!("dark")).unwrap();
    assert_eq!(f.json(), json!({ "theme": "dark", "other": 1 }));
}

#[test]
fn setting_an_invalid_value_is_refused_and_leaves_the_file_alone() {
    let f = fixture();
    f.write(r#"{ "theme": "dark" }"#);
    let refused = f.file().set("theme", json!(42)).unwrap_err();
    assert!(matches!(refused, SettingsError::InvalidValue { .. }), "{refused:?}");
    assert_eq!(f.json(), json!({ "theme": "dark" }));
}

#[test]
fn setting_an_unknown_key_is_refused() {
    let f = fixture();
    let refused = f.file().set("nope", json!(1)).unwrap_err();
    assert!(matches!(refused, SettingsError::UnknownKey { .. }), "{refused:?}");
    assert!(!f.path.exists());
}

#[test]
fn errors_reach_the_ui_as_kind_and_message() {
    let f = fixture();
    let refused = f.file().set("nope", json!(1)).unwrap_err();
    let wire = serde_json::to_value(&refused).unwrap();
    assert_eq!(wire["kind"], json!("unknownKey"));
    assert!(wire["message"].as_str().unwrap().contains("nope"));
}

#[test]
fn snapshots_serialise_for_the_ui_in_camel_case() {
    let f = fixture();
    f.write("oops");
    let wire = serde_json::to_value(f.file().load()).unwrap();
    assert_eq!(wire["values"]["theme"], json!("system"));
    assert!(wire["fileError"].is_string());
    assert!(wire["warnings"].is_object());
}
