import { useContext, useEffect, useRef, useState } from "react";
import { ArrowDown, ArrowUp, GitBranch, LoaderCircle, RefreshCw } from "lucide-react";

import { ErrorToast } from "@/components/ErrorToast";
import { Button } from "@/components/ui/button";
import { api, type GitCommand, type GitStatus, type Workspace } from "@/lib/api";
import { GitCommandsContext } from "@/lib/useGitCommands";

type GitControlsProps = {
  workspace: Workspace;
  /** A command found the folder gone: re-read the Workspaces so it shows as Missing. */
  onFoundMissing: () => void;
};

const NO_REMOTE = "This repository has no remote to use";

/** The status line and git commands in the Workspace header (spec §8.2.1). */
export function GitControls({ workspace, onFoundMissing }: GitControlsProps) {
  const [loaded, setLoaded] = useState<{ workspaceId: number; status: GitStatus } | null>(null);
  const commands = useContext(GitCommandsContext);
  const refresh = useRef<() => void>(() => {});
  const seenFinished = useRef(commands.finished);

  // On opening, on every window focus, and through `refresh`. One read at a
  // time: a request during a read gets one more read after it.
  useEffect(() => {
    if (workspace.missing) {
      setLoaded(null);
      refresh.current = () => {};
      return;
    }
    let current = true;
    let reading = false;
    let again = false;
    const workspaceId = workspace.id;
    async function read() {
      if (reading) {
        again = true;
        return;
      }
      reading = true;
      do {
        again = false;
        try {
          const status = await api.gitStatus(workspaceId);
          if (current) setLoaded(status ? { workspaceId, status } : null);
        } catch {
          if (current) setLoaded(null);
        }
      } while (again && current);
      reading = false;
    }
    refresh.current = () => void read();
    refresh.current();
    window.addEventListener("focus", refresh.current);
    const onFocus = refresh.current;
    return () => {
      current = false;
      window.removeEventListener("focus", onFocus);
    };
  }, [workspace]);

  useEffect(() => {
    if (commands.finished === seenFinished.current) return;
    seenFinished.current = commands.finished;
    refresh.current();
  }, [commands.finished]);

  const run = (command: GitCommand) => commands.run(workspace, command, onFoundMissing);
  const toast = commands.toast;

  const status = loaded?.workspaceId === workspace.id && !workspace.missing ? loaded.status : null;
  const active = commands.running.get(workspace.id);

  return (
    <>
      {status !== null && (
        <div className="flex flex-col gap-1.5">
          <GitStatusLine status={status} onRefresh={() => refresh.current()} />
          <div className="flex flex-wrap gap-2">
            <CommandButton
              label="Pull"
              blockedBy={status.remote === null ? NO_REMOTE : null}
              active={active}
              command="pull"
              onRun={run}
            />
            <CommandButton
              label="Push"
              blockedBy={
                status.remote === null ? NO_REMOTE : status.branch === null ? "HEAD is detached, so there is no branch to push" : null
              }
              active={active}
              command="push"
              onRun={run}
            />
            <CommandButton
              label={status.defaultBranch === null ? "Checkout default branch" : `Checkout ${status.defaultBranch}`}
              blockedBy={
                status.defaultBranch === null
                  ? "No default branch was found"
                  : status.defaultBranch === status.branch
                    ? `Already on ${status.branch}`
                    : null
              }
              active={active}
              command="checkoutDefault"
              onRun={run}
            />
          </div>
        </div>
      )}
      {toast !== null && (
        <ErrorToast
          message={toast.workspaceId === workspace.id ? toast.message : `${toast.workspaceName}: ${toast.message}`}
          {...(toast.retry !== undefined && {
            actionLabel: "Run in terminal",
            onAction: () => void commands.runInTerminal(toast),
          })}
          onDismiss={commands.dismissToast}
        />
      )}
    </>
  );
}

function GitStatusLine({ status, onRefresh }: { status: GitStatus; onRefresh: () => void }) {
  return (
    <div className="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
      <span className="flex items-center gap-1">
        <GitBranch aria-hidden className="size-3.5" />
        {status.branch === null ? "Detached HEAD" : <span className="font-mono">{status.branch}</span>}
      </span>
      {status.tracking !== null && (
        <span
          role="img"
          className="flex items-center gap-1"
          aria-label={`${status.tracking.ahead} ahead, ${status.tracking.behind} behind`}
        >
          <ArrowUp aria-hidden className="size-3" />
          {status.tracking.ahead}
          <ArrowDown aria-hidden className="size-3" />
          {status.tracking.behind}
        </span>
      )}
      {status.branch !== null && status.tracking === null && <span>No upstream</span>}
      {status.changes && <span className="text-brand">Uncommitted changes</span>}
      <Button
        size="icon"
        variant="ghost"
        className="size-6"
        title="Refresh git status"
        aria-label="Refresh git status"
        onClick={onRefresh}
      >
        <RefreshCw className="size-3" />
      </Button>
    </div>
  );
}

function CommandButton({
  label,
  blockedBy,
  active,
  command,
  onRun,
}: {
  label: string;
  /** Why it can't run now, shown as its tooltip. */
  blockedBy: string | null;
  /** The command running in this Workspace, if any. */
  active: GitCommand | undefined;
  command: GitCommand;
  onRun: (command: GitCommand) => Promise<void>;
}) {
  const isRunning = active === command;
  return (
    // A disabled button gets no hover events, so the tooltip sits on a wrapper.
    <span title={blockedBy ?? undefined}>
      <Button
        size="sm"
        variant="outline"
        disabled={blockedBy !== null || active !== undefined}
        aria-busy={isRunning}
        onClick={() => void onRun(command)}
      >
        {isRunning && <LoaderCircle aria-hidden className="animate-spin" />}
        {label}
      </Button>
    </span>
  );
}
