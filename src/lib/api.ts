// The UI's only door to the backend (ADR 0001): typed wrappers over the Rust
// commands, plus the native folder picker. Components import `api`; tests mock
// this module.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export type Workspace = {
  id: number;
  name: string;
  /** Normalised path, displayed as stored (spec §6.1). */
  path: string;
  pinned: boolean;
  note: string | null;
  /** Unix epoch milliseconds. */
  createdAt: number;
  /** In the order added; unique case-insensitively (spec §6.2). */
  tags: string[];
  /** Paths it was re-pointed away from, oldest first; they keep attributing Sessions (spec §6.5). */
  formerPaths: string[];
  /** The directory did not exist when the Workspace was last read (spec §6.3). */
  missing: boolean;
};

/**
 * A Workspace saved at a path. `movedFrom` names the Workspace that path was
 * moved off, when it was one of its former paths (spec §6.2).
 */
export type Saved = {
  workspace: Workspace;
  movedFrom: { workspaceId: number; workspaceName: string } | null;
};

/** The edit dialog's fields. A blank note is saved as none. */
export type WorkspaceEdit = { name: string; note: string; tags: string[] };

export type WorkspaceError =
  | { kind: "emptyName"; message: string }
  | { kind: "noSuchWorkspace"; message: string }
  | { kind: "invalidPath"; message: string }
  | CommandError;

/** A directory with Sessions that no Workspace covers (spec §5.5). */
export type Suggestion = {
  /** The Sessions' normalised first `cwd`. */
  path: string;
  sessionCount: number;
  /** Newest last activity, Unix epoch milliseconds. */
  lastSeen: number | null;
  /** The directory is gone; it can only be dismissed. */
  missing: boolean;
};

/** Every command error carries a `kind` and a human-readable `message`. */
export type CommandError = { kind: "internal"; message: string };

export type AddWorkspaceError =
  | { kind: "emptyName"; message: string }
  | { kind: "invalidPath"; message: string }
  | { kind: "notFound"; message: string; path: string }
  | { kind: "notADirectory"; message: string; path: string }
  | { kind: "pathTaken"; message: string; workspaceId: number; workspaceName: string }
  | { kind: "noSuchWorkspace"; message: string }
  | CommandError;

/**
 * Every setting by its dotted `settings.json` key (spec §9.1). Mirrors `DEFS`
 * in `src-tauri/src/settings.rs`; add a setting in both places.
 */
export type SettingValues = {
  theme: "system" | "light" | "dark";
  /** Non-Windows platforms offer only `custom` (see `choices`). */
  "terminal.preset": "windowsTerminal" | "console" | "custom";
  "terminal.windowsTerminal.target": "newWindow" | "newTab";
  /** The custom argv template; must contain `{command}` as an argument. */
  "terminal.custom": string[] | null;
  /** Blank: `claude` on PATH (spec §5.3). */
  "agents.claude-code.executable": string;
  /** Blank: `CLAUDE_CONFIG_DIR`, else `~/.claude` (spec §5.3). */
  "agents.claude-code.transcriptRoot": string;
};

export type SettingKey = keyof SettingValues;

/** The effective settings: defaults filled in, invalid values replaced. */
export type SettingsSnapshot = {
  values: SettingValues;
  /** Key -> why the file's value was ignored (shown inline). */
  warnings: Partial<Record<SettingKey, string>>;
  /** Why the whole file was ignored; saves are refused until fixed or reset. */
  fileError: string | null;
  /** Choice setting -> the values offered on this platform. */
  choices: Partial<Record<SettingKey, string[]>>;
};

export type SettingsError =
  | { kind: "unknownKey"; message: string; key: string }
  | { kind: "invalidValue"; message: string; key: string }
  | { kind: "fileInvalid"; message: string }
  | CommandError;

/** What an Agent supports; a control is hidden when its flag is off (spec §3.2). */
export type Capabilities = { continue: boolean; resume: boolean; sessionHistory: boolean };

export type AgentStatus = {
  id: string;
  displayName: string;
  capabilities: Capabilities;
  /** The resolved executable, or `null` when it is not found. */
  executable: string | null;
};

/** Settings › Agents › Claude Code status lines (spec §9.3). */
export type ClaudeCodeStatus = {
  executable: { state: "found"; path: string; version: string | null } | { state: "notFound" };
  transcriptRoot:
    | { state: "ok"; path: string }
    | { state: "noProjects"; path: string }
    /** An empty path: no home folder is known. */
    | { state: "notFound"; path: string };
};

export type LaunchMode = "new" | "continue";

export type LaunchError =
  | { kind: "executableNotFound"; message: string; name: string }
  | { kind: "workspaceNotFound"; message: string; workspaceId: number }
  | { kind: "workspaceMissing"; message: string; workspaceId: number; path: string }
  | { kind: "spawnFailed"; message: string; program: string; osError: string }
  | { kind: "terminalNotConfigured"; message: string }
  | CommandError;

export type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };

/** Normalise a rejected `invoke` into a `{ kind, message }` error. */
function toError<E extends { kind: string; message: string }>(raw: unknown): E {
  if (raw && typeof raw === "object" && "kind" in raw && "message" in raw) return raw as E;
  return { kind: "internal", message: String(raw) } as E;
}

async function call<T, E extends { kind: string; message: string }>(
  command: string,
  args?: Record<string, unknown>,
): Promise<Result<T, E>> {
  try {
    return { ok: true, value: await invoke<T>(command, args) };
  } catch (raw) {
    return { ok: false, error: toError<E>(raw) };
  }
}

export const api = {
  /** All Workspaces. Throws on an internal error. */
  async listWorkspaces(): Promise<Workspace[]> {
    const result = await call<Workspace[], CommandError>("list_workspaces");
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  },

  addWorkspace(name: string, path: string): Promise<Result<Saved, AddWorkspaceError>> {
    return call("add_workspace", { name, path });
  },

  /** Locate… or a path edit: the old path becomes a former path (spec §6.5). The folder must exist. */
  repointWorkspace(id: number, path: string): Promise<Result<Saved, AddWorkspaceError>> {
    return call("repoint_workspace", { id, path });
  },

  removeFormerPath(id: number, path: string): Promise<Result<Workspace, WorkspaceError>> {
    return call("remove_former_path", { id, path });
  },

  updateWorkspace(id: number, edit: WorkspaceEdit): Promise<Result<Workspace, WorkspaceError>> {
    return call("update_workspace", { id, ...edit });
  },

  setPinned(id: number, pinned: boolean): Promise<Result<Workspace, WorkspaceError>> {
    return call("set_workspace_pinned", { id, pinned });
  },

  /** Deletes the Workspace's row and tags (spec §4.3). */
  removeWorkspace(id: number): Promise<Result<null, WorkspaceError>> {
    return call("remove_workspace", { id });
  },

  /** Re-read settings.json; the backend also applies it (theme). */
  getSettings(): Promise<Result<SettingsSnapshot, SettingsError>> {
    return call("get_settings");
  },

  /** Save one setting (autosave); returns the new snapshot. */
  setSetting<K extends SettingKey>(
    key: K,
    value: SettingValues[K],
  ): Promise<Result<SettingsSnapshot, SettingsError>> {
    return call("set_setting", { key, value });
  },

  /** Replace settings.json with the defaults, even when it is invalid. */
  resetSettings(): Promise<Result<SettingsSnapshot, SettingsError>> {
    return call("reset_settings");
  },

  /** The Agent's identity, capabilities and executable, resolved now. Throws on an internal error. */
  async agentStatus(): Promise<AgentStatus> {
    const result = await call<AgentStatus, CommandError>("agent_status");
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  },

  /** Open a terminal running the Agent in the Workspace's directory. */
  launchSession(workspaceId: number, mode: LaunchMode): Promise<Result<void, LaunchError>> {
    return call("launch_session", { workspaceId, mode });
  },

  /** Suggestions computed from the stored Sessions. Throws on an internal error. */
  async listSuggestions(): Promise<Suggestion[]> {
    const result = await call<Suggestion[], CommandError>("list_suggestions");
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  },

  /** Hide a Suggestion's path permanently (spec §6.6). */
  dismissPath(path: string): Promise<Result<null, WorkspaceError>> {
    return call("dismiss_path", { path });
  },

  /** Start a background transcript scan; progress arrives through `onScanState`. */
  async startScan(): Promise<void> {
    await call("start_scan");
  },

  /** Whether a scan is running now. */
  async scanRunning(): Promise<boolean> {
    const result = await call<boolean, CommandError>("scan_running");
    return result.ok && result.value;
  },

  /** Calls `listener(true)` when a scan starts and `(false)` when it finishes. Resolves to an unsubscribe. */
  onScanState(listener: (running: boolean) => void): Promise<() => void> {
    return listen<boolean>("scan-state", (event) => listener(event.payload));
  },

  /** Settings › Terminal › Test: run `claude --version` in the home directory through the template. */
  testTerminal(template: string[]): Promise<Result<void, LaunchError>> {
    return call("test_terminal", { template });
  },

  /**
   * The Claude Code status lines. The executable's `--version` probe is
   * cached; `refresh` re-probes it (an override change always re-probes).
   */
  async claudeCodeStatus(refresh: boolean): Promise<ClaudeCodeStatus> {
    const result = await call<ClaudeCodeStatus, CommandError>("claude_code_status", { refresh });
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  },

  /** Settings › Suggestions: the dismissed paths, oldest first. */
  listDismissedPaths(): Promise<Result<string[], CommandError>> {
    return call("list_dismissed_paths");
  },

  /** Un-dismiss a path so it can reappear as a Suggestion. */
  restoreDismissedPath(path: string): Promise<Result<null, WorkspaceError>> {
    return call("restore_dismissed_path", { path });
  },

  /** Re-parse every transcript on disk; progress arrives through `onScanState`. */
  async rescanTranscripts(): Promise<void> {
    await call("rescan_transcripts");
  },

  /** Show the app data folder in the system file manager. */
  openDataFolder(): Promise<Result<null, CommandError>> {
    return call("open_data_folder");
  },

  /** Native folder picker; `null` when cancelled. */
  async pickFolder(): Promise<string | null> {
    const picked = await open({ directory: true, multiple: false });
    return typeof picked === "string" ? picked : null;
  },
};

/** The last path segment, for pre-filling a Workspace name. */
export function lastSegment(path: string): string {
  const segments = path.split(/[\\/]+/).filter(Boolean);
  return segments.at(-1) ?? path;
}
