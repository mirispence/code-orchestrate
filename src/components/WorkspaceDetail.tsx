import { useEffect, useId, useState } from "react";
import { Play, RotateCw, Star } from "lucide-react";

import { ErrorToast } from "@/components/ErrorToast";
import { ResumeTable } from "@/components/ResumeTable";
import { StatTiles } from "@/components/StatTiles";
import { TagList } from "@/components/TagList";
import { Button } from "@/components/ui/button";
import { api, type AgentStatus, type LaunchMode, type Session, type Workspace, type WorkspaceStats } from "@/lib/api";
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
  const [launchError, setLaunchError] = useState<string | null>(null);
  const [sessions, setSessions] = useState<{ workspaceId: number; list: Session[] } | null>(null);
  const [sessionsError, setSessionsError] = useState<string | null>(null);
  const hintId = useId();

  // Resolved each time a Workspace is shown, so installing Claude Code is
  // picked up without restarting.
  useEffect(() => {
    let current = true;
    api.agentStatus().then(
      (status) => {
        if (!current) return;
        setAgent(status);
        setStatusError(null);
      },
      (e: unknown) => {
        if (current) setStatusError(e instanceof Error ? e.message : String(e));
      },
    );
    return () => {
      current = false;
    };
  }, [workspace.id]);

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
    setLaunchError(null);
    const result = await api.launchSession(workspace.id, mode);
    setLaunching(false);
    if (result.ok) return;
    // The backend re-checks the folder on every click; the banner then explains it.
    if (result.error.kind === "workspaceMissing") onFoundMissing();
    else setLaunchError(result.error.message);
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
    <div className="flex flex-col gap-4 p-6">
      <WorkspaceHeader workspace={workspace} onEdit={onEdit} onTogglePin={onTogglePin} />

      {workspace.missing && (
        <div className="flex items-center justify-between gap-4 rounded-md border border-brand/40 bg-brand/10 p-3 text-sm">
          <p className="min-w-0 break-all">
            Folder not found: <span className="font-mono">{workspace.path}</span>. Its stats and Sessions are kept.
          </p>
          <Button size="sm" variant="outline" className="shrink-0" onClick={onLocate}>
            Locate…
          </Button>
        </div>
      )}

      {/* A disabled button gets no hover events, so the tooltip sits on a wrapper. */}
      <div className="flex gap-2" title={hint ?? undefined}>
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

      {agent?.capabilities.sessionHistory !== false && agent?.capabilities.resume !== false && (
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
            onResume={(sessionId) => void launch({ resume: sessionId })}
          />
        </>
      )}

      {launchError !== null && (
        <ErrorToast
          message={launchError}
          actionLabel="Open Settings"
          onAction={onOpenSettings}
          onDismiss={() => setLaunchError(null)}
        />
      )}
    </div>
  );
}

function WorkspaceHeader({
  workspace,
  onEdit,
  onTogglePin,
}: Pick<WorkspaceDetailProps, "workspace" | "onEdit" | "onTogglePin">) {
  return (
    <header className="flex flex-col gap-2">
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-1">
          <h2 className="text-xl font-semibold break-words">{workspace.name}</h2>
          <p className="font-mono text-sm break-all text-muted-foreground">{workspace.path}</p>
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
      {workspace.note && <p className="text-sm whitespace-pre-wrap">{workspace.note}</p>}
    </header>
  );
}
