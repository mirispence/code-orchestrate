//! The exact `{ kind, message, ...fields }` JSON each command error sends the
//! UI (`src/lib/api.ts` types it). Every variant is listed, so a change to
//! how errors serialise shows up here.
use code_orchestrate_lib::launch::LaunchError;
use code_orchestrate_lib::workspaces::{AddWorkspaceError, WorkspaceError};
use serde_json::{Value, json};

fn wire(e: &impl serde::Serialize) -> Value {
    serde_json::to_value(e).unwrap()
}

#[test]
fn launch_errors() {
    let cases = [
        (
            LaunchError::ExecutableNotFound { name: "claude".into() },
            json!({"kind": "executableNotFound", "message": "claude was not found", "name": "claude"}),
        ),
        (
            LaunchError::WorkspaceNotFound { workspace_id: 3 },
            json!({"kind": "workspaceNotFound", "message": "Workspace 3 does not exist", "workspaceId": 3}),
        ),
        (
            LaunchError::WorkspaceMissing { workspace_id: 3, path: r"E:\gone".into() },
            json!({
                "kind": "workspaceMissing",
                "message": r"Folder not found: E:\gone. Locate it to launch here.",
                "workspaceId": 3,
                "path": r"E:\gone",
            }),
        ),
        (
            LaunchError::SessionNotFound { session_id: "abc".into() },
            json!({"kind": "sessionNotFound", "message": "Session abc was not found", "sessionId": "abc"}),
        ),
        (
            LaunchError::TranscriptDeleted { session_id: "abc".into() },
            json!({
                "kind": "transcriptDeleted",
                "message": "Transcript deleted: Session abc can no longer be resumed",
                "sessionId": "abc",
            }),
        ),
        (
            LaunchError::SpawnFailed { program: "wt.exe".into(), os_error: "denied".into() },
            json!({
                "kind": "spawnFailed",
                "message": "Could not start wt.exe: denied",
                "program": "wt.exe",
                "osError": "denied",
            }),
        ),
        (
            LaunchError::TerminalNotConfigured { message: "Set a template".into() },
            json!({"kind": "terminalNotConfigured", "message": "Set a template"}),
        ),
        (LaunchError::Internal("boom".into()), json!({"kind": "internal", "message": "boom"})),
    ];
    for (error, expected) in cases {
        assert_eq!(wire(&error), expected, "{error:?}");
    }
}

#[test]
fn add_workspace_errors() {
    let cases = [
        (AddWorkspaceError::EmptyName, json!({"kind": "emptyName", "message": "A Workspace needs a name"})),
        (AddWorkspaceError::InvalidPath("bad".into()), json!({"kind": "invalidPath", "message": "bad"})),
        (
            AddWorkspaceError::NotFound { path: r"E:\x".into() },
            json!({"kind": "notFound", "message": r"Folder not found: E:\x", "path": r"E:\x"}),
        ),
        (
            AddWorkspaceError::NotADirectory { path: r"E:\f".into() },
            json!({"kind": "notADirectory", "message": r"Not a folder: E:\f", "path": r"E:\f"}),
        ),
        (
            AddWorkspaceError::PathTaken { workspace_id: 7, workspace_name: "Code".into() },
            json!({
                "kind": "pathTaken",
                "message": "Already a Workspace: Code",
                "workspaceId": 7,
                "workspaceName": "Code",
            }),
        ),
        (
            AddWorkspaceError::NoSuchWorkspace,
            json!({"kind": "noSuchWorkspace", "message": "That Workspace no longer exists"}),
        ),
        (AddWorkspaceError::Internal("boom".into()), json!({"kind": "internal", "message": "boom"})),
    ];
    for (error, expected) in cases {
        assert_eq!(wire(&error), expected, "{error:?}");
    }
}

#[test]
fn workspace_errors() {
    let cases = [
        (WorkspaceError::EmptyName, json!({"kind": "emptyName", "message": "A Workspace needs a name"})),
        (
            WorkspaceError::NoSuchWorkspace,
            json!({"kind": "noSuchWorkspace", "message": "That Workspace no longer exists"}),
        ),
        (WorkspaceError::InvalidPath("bad".into()), json!({"kind": "invalidPath", "message": "bad"})),
        (WorkspaceError::Internal("boom".into()), json!({"kind": "internal", "message": "boom"})),
    ];
    for (error, expected) in cases {
        assert_eq!(wire(&error), expected, "{error:?}");
    }
}
