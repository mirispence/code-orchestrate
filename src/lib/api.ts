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
  /** In the order added; unique case-insensitively (spec §6.2). */
  tags: string[];
};

/** The edit dialog's fields. A blank note is saved as none. */
export type WorkspaceEdit = { name: string; note: string; tags: string[] };

export type WorkspaceError =
  | { kind: "emptyName"; message: string }
  | { kind: "noSuchWorkspace"; message: string }
  | CommandError;

/** Every command error carries a `kind` and a human-readable `message`. */
export type CommandError = { kind: "internal"; message: string };

export type AddWorkspaceError =
  | { kind: "emptyName"; message: string }
  | { kind: "invalidPath"; message: string }
  | { kind: "notFound"; message: string; path: string }
  | { kind: "notADirectory"; message: string; path: string }
  | { kind: "pathTaken"; message: string; workspaceId: number; workspaceName: string }
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
