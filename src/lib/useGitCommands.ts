import { createContext, useCallback, useMemo, useState } from "react";

import { api, type GitCommand, type GitCommandError, type Workspace } from "@/lib/api";

export type GitToast = {
  workspaceId: number;
  workspaceName: string;
  message: string;
  /** What Run in terminal repeats; absent when it isn't offered. */
  retry?: GitCommand;
};

export type GitCommands = {
  /** The command running in each Workspace that has one. */
  running: ReadonlyMap<number, GitCommand>;
  /** Counts finished commands, so an open status line knows to reread. */
  finished: number;
  toast: GitToast | null;
  run: (workspace: Workspace, command: GitCommand, onFoundMissing: () => void) => Promise<void>;
  runInTerminal: (toast: GitToast) => Promise<void>;
  dismissToast: () => void;
};

/** What git said, as opposed to the backend refusing before running it. */
function ranAndFailed(error: GitCommandError) {
  return error.kind === "failed" || error.kind === "timedOut";
}

/**
 * Git commands in progress (spec §8.2.1). Held above the Workspace detail,
 * which unmounts when Settings or Suggestions opens while a command carries on.
 */
export function useGitCommands(): GitCommands {
  const [running, setRunning] = useState<ReadonlyMap<number, GitCommand>>(new Map());
  const [finished, setFinished] = useState(0);
  const [toast, setToast] = useState<GitToast | null>(null);

  const run = useCallback(async (workspace: Workspace, command: GitCommand, onFoundMissing: () => void) => {
    const { id: workspaceId, name: workspaceName } = workspace;
    setRunning((now) => new Map(now).set(workspaceId, command));
    setToast(null);
    const result = await api.runGitCommand(workspaceId, command);
    setRunning((now) => {
      const next = new Map(now);
      next.delete(workspaceId);
      return next;
    });
    setFinished((count) => count + 1);
    if (result.ok) return;
    if (result.error.kind === "workspaceMissing") {
      onFoundMissing();
      return;
    }
    const offerTerminal = command !== "checkoutDefault" && ranAndFailed(result.error);
    setToast({ workspaceId, workspaceName, message: result.error.message, ...(offerTerminal && { retry: command }) });
  }, []);

  const runInTerminal = useCallback(async ({ workspaceId, workspaceName, retry }: GitToast) => {
    if (retry === undefined) return;
    setToast(null);
    const result = await api.runGitInTerminal(workspaceId, retry);
    if (!result.ok) setToast({ workspaceId, workspaceName, message: result.error.message });
  }, []);

  const dismissToast = useCallback(() => setToast(null), []);

  return useMemo(
    () => ({ running, finished, toast, run, runInTerminal, dismissToast }),
    [running, finished, toast, run, runInTerminal, dismissToast],
  );
}

/** Idle until the app provides the real one. */
export const GitCommandsContext = createContext<GitCommands>({
  running: new Map(),
  finished: 0,
  toast: null,
  run: async () => {},
  runInTerminal: async () => {},
  dismissToast: () => {},
});
