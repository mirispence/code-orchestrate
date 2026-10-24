//! The webview is a pure view (ADR 0001, spec §3.1): it may reach only the
//! app's own commands, the core IPC plumbing, and the dialog plugin.

use std::path::PathBuf;

use serde_json::Value;

fn read_json(relative: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("invalid JSON in {}: {e}", path.display()))
}

fn permission_ids(capability: &Value) -> Vec<String> {
    capability["permissions"]
        .as_array()
        .expect("capability has a permissions array")
        .iter()
        .map(|p| match p {
            Value::String(id) => id.clone(),
            Value::Object(o) => o["identifier"].as_str().expect("permission identifier").to_owned(),
            other => panic!("unexpected permission entry: {other}"),
        })
        .collect()
}

#[test]
fn app_identifier_is_com_mirispence_code_orchestrate() {
    let conf = read_json("tauri.conf.json");
    assert_eq!(conf["identifier"], "com.mirispence.code-orchestrate");
}

#[test]
fn webview_capabilities_allow_only_core_and_dialog() {
    let capabilities_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("capabilities");
    let files: Vec<_> = std::fs::read_dir(&capabilities_dir)
        .expect("capabilities directory exists")
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "expected exactly one capability file, found {files:?}");

    let capability = read_json("capabilities/default.json");
    let ids = permission_ids(&capability);

    assert!(ids.iter().any(|id| id.starts_with("dialog:")), "dialog plugin must be allowed");
    for id in &ids {
        assert!(
            id.starts_with("core:") || id.starts_with("dialog:"),
            "permission {id} is outside the allowed set (core IPC and dialog only)"
        );
        for forbidden in ["shell", "fs", "process", "opener"] {
            assert!(!id.contains(forbidden), "permission {id} grants {forbidden} access");
        }
    }
}
