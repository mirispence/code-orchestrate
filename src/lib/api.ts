// The UI's only door to the backend (ADR 0001): typed wrappers over the Rust
// commands, plus the native folder picker. Components import `api`; tests mock
// this module.
import { invoke } from "@tauri-apps/api/core";
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
};

/** Every command error carries a `kind` and a human-readable `message`. */
export type CommandError = { kind: "internal"; message: string };

export type AddWorkspaceError =
  | { kind: "emptyName"; message: string }
  | { kind: "invalidPath"; message: string }
  | { kind: "notFound"; message: string; path: string }
  | { kind: "notADirectory"; message: string; path: string }
  | { kind: "pathTaken"; message: string; workspaceId: number; workspaceName: string }
  | CommandError;

/**
 * Every setting by its dotted `settings.json` key (spec §9.1). Mirrors `DEFS`
 * in `src-tauri/src/settings.rs`; add a setting in both places.
 */
export type SettingValues = {
  theme: "system" | "light" | "dark";
};

export type SettingKey = keyof SettingValues;

/** The effective settings: defaults filled in, invalid values replaced. */
export type SettingsSnapshot = {
  values: SettingValues;
  /** Key -> why the file's value was ignored (shown inline). */
  warnings: Partial<Record<SettingKey, string>>;
  /** Why the whole file was ignored; saves are refused until fixed or reset. */
  fileError: string | null;
};

export type SettingsError =
  | { kind: "unknownKey"; message: string; key: string }
  | { kind: "invalidValue"; message: string; key: string }
  | { kind: "fileInvalid"; message: string }
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

  addWorkspace(name: string, path: string): Promise<Result<Workspace, AddWorkspaceError>> {
    return call("add_workspace", { name, path });
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
