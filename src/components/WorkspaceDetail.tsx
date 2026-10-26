import { useEffect, useId, useState } from "react";
import { GitBranch, Play, RotateCw, Star } from "lucide-react";

import { ErrorToast } from "@/components/ErrorToast";
import { ResumeTable } from "@/components/ResumeTable";
import { StatTiles } from "@/components/StatTiles";
import { TagList } from "@/components/TagList";
import { Button } from "@/components/ui/button";
import {
  api,
  type AgentStatus,
  type LaunchMode,
  type Repository,
  type Session,
  type Workspace,
  type WorkspaceStats,
} from "@/lib/api";
import { cn } from "@/lib/utils";

type WorkspaceDetailProps = {
  workspace: Workspace;
  /** This Workspace's stats; `null` or absent until loaded (shown as empty). */
  stats?: WorkspaceStats | null;
  onEdit: () => void;
  onTogglePin: () => void;
  /** Opens the Settings page (spec §9), where the executable can be set. */
  onOpenSettings: () => void;
  /** Locate…: pick the folder's new location and re-point the Workspace (spec §6.5). */
  onLocate: () => void;
  /** A launch found the folder gone: re-read the Workspaces so it shows as Missing. */
  onFoundMissing: () => void;
};

const MISSING_HINT = "The folder is missing. Locate it to launch here.";

/** Stats for a Workspace with no Sessions. */
function emptyStats(workspaceId: number): WorkspaceStats {
  return {
    workspaceId,
    sessionCount: 0,
    lastUsed: null,
    sessionTimeMs: null,
    tokens: { input: null, output: null, cacheWrite: null, cacheRead: null },
  };
}

/** Detail pane for the selected Workspace (spec §8.2). */
export function WorkspaceDetail({
  workspace,
  stats,
  onEdit,
  onTogglePin,
  onOpenSettings,
  onLocate,
  onFoundMissing,
}: WorkspaceDetailProps) {
  const [agent, setAgent] = useState<AgentStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [launching, setLaunching] = useState(false);
  // One toast at a time; a launch failure points to Settings, a repository one does not.
  const [toast, setToast] = useState<{ message: string; openSettings: boolean } | null>(null);
  const [repository, setRepository] = useState<{ workspaceId: number; value: Repository } | null>(null);
  const [sessions, setSessions] = useState<{ workspaceId: number; list: Session[] } | null>(null);
  const [sessionsError, setSessionsError] = useState<string | null>(null);
  const hintId = useId();

  // Resolved on each Workspace reload (selecting, refresh, a finished scan) and
  // on window focus, so installing Claude Code is picked up without restarting.
  // A reload hands over a new `workspace` object, even for the same id.
  useEffect(() => {
    let current = true;
    // Checks can overlap (focus during a reload); only the newest one applies.
    let sent = 0;
    function check() {
      const id = ++sent;
      api.agentStatus().then(
        (status) => {
          if (!current || id !== sent) return;
          setAgent(status);
          setStatusError(null);
        },
        (e: unknown) => {
          if (current && id === sent) setStatusError(e instanceof Error ? e.message : String(e));
        },
      );
    }
    check();
    window.addEventListener("focus", check);
    return () => {
      current = false;
      window.removeEventListener("focus", check);
    };
  }, [workspace]);

  // Resolved on each Workspace reload, like the Agent status. Any failure, or
  // a slow git, just leaves it hidden (#32).
  useEffect(() => {
    if (workspace.missing) {
      setRepository(null);
      return;
    }
    let current = true;
    const workspaceId = workspace.id;
    // `then` also turns a synchronous throw into a rejection.
    Promise.resolve()
      .then(() => api.workspaceRepository(workspaceId))
      .then(
        (value) => {
          if (current) setRepository(value ? { workspaceId, value } : null);
        },
        () => {
          if (current) setRepository(null);
        },
      );
    return () => {
      current = false;
    };
  }, [workspace]);

  // Re-read whenever the stats are (each reload after a scan or an edit).
  useEffect(() => {
    let current = true;
    const workspaceId = workspace.id;
    api.listWorkspaceSessions(workspaceId).then(
      (result) => {
        if (!current) return;
        if (result.ok) {
          setSessions({ workspaceId, list: result.value });
          setSessionsError(null);
        } else setSessionsError(result.error.message);
      },
      (e: unknown) => {
        if (current) setSessionsError(e instanceof Error ? e.message : String(e));
      },
    );
    return () => {
      current = false;
    };
  }, [workspace.id, stats]);

  async function launch(mode: LaunchMode) {
    setLaunching(true);
    setToast(null);
    const result = await api.launchSession(workspace.id, mode);
    setLaunching(false);
    if (result.ok) return;
    // The backend re-checks the folder on every click; the banner then explains it.
    if (result.error.kind === "workspaceMissing") onFoundMissing();
    else setToast({ message: result.error.message, openSettings: true });
  }

  async function openRepository() {
    setToast(null);
    const result = await api.openWorkspaceRepository(workspace.id);
    if (result.ok) return;
    if (result.error.kind === "workspaceMissing") onFoundMissing();
    else setToast({ message: result.error.message, openSettings: false });
  }

  const name = agent?.displayName ?? "Claude Code";
  const hint = workspace.missing
    ? MISSING_HINT
    : statusError !== null
      ? `Could not look for ${name}: ${statusError}`
      : agent !== null && agent.executable === null
        ? `${name} was not found on PATH. Set its location in Settings.`
        : null;
  const canLaunch = !workspace.missing && agent !== null && agent.executable !== null && !launching;
  const describedBy = hint !== null ? hintId : undefined;

  return (
    <div className="flex max-w-5xl flex-col gap-5 px-9 pt-7 pb-12">
      <WorkspaceHeader
        workspace={workspace}
        repository={repository?.workspaceId === workspace.id ? repository.value : null}
        onOpenRepository={() => void openRepository()}
        onEdit={onEdit}
        onTogglePin={onTogglePin}
      />

      {workspace.missing && (
        <div className="flex items-center justify-between gap-4 rounded-lg border border-brand/40 bg-brand-soft p-3 text-sm">
          <p className="min-w-0 break-all">
            Folder not found: <span className="font-mono">{workspace.path}</span>. Its stats and Sessions are kept.
          </p>
          <Button size="sm" variant="outline" className="shrink-0" onClick={onLocate}>
            Locate…
          </Button>
        </div>
      )}

      {/* A disabled button gets no hover events, so the tooltip sits on a wrapper. */}
      <div className="flex gap-2 pb-1" title={hint ?? undefined}>
        <Button
          className="bg-brand text-brand-foreground hover:bg-brand/90"
          disabled={!canLaunch}
          aria-describedby={describedBy}
          onClick={() => void launch("new")}
        >
          <Play />
          New Session
        </Button>
        {agent?.capabilities.continue !== false && (
          <Button
            variant="outline"
            disabled={!canLaunch}
            aria-describedby={describedBy}
            onClick={() => void launch("continue")}
          >
            <RotateCw />
            Continue last
          </Button>
        )}
        {hint !== null && (
          <span id={hintId} className="sr-only">
            {hint}
          </span>
        )}
      </div>

      {/* Hidden when the Agent keeps no Session history (spec §3.2). */}
      {agent?.capabilities.sessionHistory !== false && <StatTiles stats={stats ?? emptyStats(workspace.id)} />}

      {/* Without resume the Sessions still list, just with no Resume column (spec §3.2). */}
      {agent?.capabilities.sessionHistory !== false && (
        <>
          {sessionsError !== null && (
            <p role="alert" className="text-sm text-destructive">
              Could not load Sessions: {sessionsError}
            </p>
          )}
          <ResumeTable
            sessions={sessions?.workspaceId === workspace.id ? sessions.list : []}
            canLaunch={canLaunch}
            hintId={describedBy}
            onResume={
              agent?.capabilities.resume === false ? undefined : (sessionId) => void launch({ resume: sessionId })
            }
          />
        </>
      )}

      {toast !== null && (
        <ErrorToast
          message={toast.message}
          {...(toast.openSettings && { actionLabel: "Open Settings", onAction: onOpenSettings })}
          onDismiss={() => setToast(null)}
        />
      )}
    </div>
  );
}

function WorkspaceHeader({
  workspace,
  repository,
  onOpenRepository,
  onEdit,
  onTogglePin,
}: Pick<WorkspaceDetailProps, "workspace" | "onEdit" | "onTogglePin"> & {
  repository: Repository | null;
  onOpenRepository: () => void;
}) {
  return (
    <header className="flex flex-col gap-2">
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-1">
          <h2 className="flex items-center gap-2 text-2xl font-semibold tracking-tight break-words">
            {/* Decorative: the Pin/Unpin button carries the state, so the heading is just the name. */}
            {workspace.pinned && <Star aria-hidden className="size-5 shrink-0 fill-brand text-brand" />}
            <span className="min-w-0">{workspace.name}</span>
          </h2>
          <p className="font-mono text-xs break-all text-muted-foreground">{workspace.path}</p>
          {repository !== null && (
            // A button, not a link: the backend opens the page (ADR 0001).
            <button
              type="button"
              title={`Open ${repository.url}`}
              className="flex w-fit items-center gap-1 text-xs text-brand underline-offset-2 hover:underline focus-visible:underline focus-visible:outline-none"
              onClick={onOpenRepository}
            >
              <GitBranch aria-hidden className="size-3.5" />
              {repository.name}
            </button>
          )}
        </div>
        <div className="flex shrink-0 gap-1">
          <Button size="sm" variant="ghost" onClick={onTogglePin}>
            <Star className={cn(workspace.pinned && "fill-brand text-brand")} />
            {workspace.pinned ? "Unpin" : "Pin"}
          </Button>
          <Button size="sm" variant="ghost" onClick={onEdit}>
            Edit
          </Button>
        </div>
      </div>
      <TagList tags={workspace.tags} />
      {workspace.note && <p className="mt-1 text-sm whitespace-pre-wrap">{workspace.note}</p>}
    </header>
  );
}
