import { useEffect, useId, useState } from "react";
import { Play, RotateCw, Star } from "lucide-react";

import { ErrorToast } from "@/components/ErrorToast";
import { TagList } from "@/components/TagList";
import { Button } from "@/components/ui/button";
import { api, type AgentStatus, type LaunchMode, type Workspace } from "@/lib/api";
import { cn } from "@/lib/utils";

type WorkspaceDetailProps = {
  workspace: Workspace;
  onEdit: () => void;
  onTogglePin: () => void;
  /** Opens the Settings page (spec §9), where the executable can be set. */
  onOpenSettings: () => void;
};

/** Detail pane for the selected Workspace (spec §8.2). Later tickets add stats and history. */
export function WorkspaceDetail({
  workspace,
  onEdit,
  onTogglePin,
  onOpenSettings,
}: WorkspaceDetailProps) {
  const [agent, setAgent] = useState<AgentStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [launching, setLaunching] = useState(false);
  const [launchError, setLaunchError] = useState<string | null>(null);
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

  async function launch(mode: LaunchMode) {
    setLaunching(true);
    setLaunchError(null);
    const result = await api.launchSession(workspace.id, mode);
    setLaunching(false);
    if (!result.ok) setLaunchError(result.error.message);
  }

  const name = agent?.displayName ?? "Claude Code";
  const hint =
    statusError !== null
      ? `Could not look for ${name}: ${statusError}`
      : agent !== null && agent.executable === null
        ? `${name} was not found on PATH. Set its location in Settings.`
        : null;
  const canLaunch = agent !== null && agent.executable !== null && !launching;
  const describedBy = hint !== null ? hintId : undefined;

  return (
    <div className="flex flex-col gap-4 p-6">
      <WorkspaceHeader workspace={workspace} onEdit={onEdit} onTogglePin={onTogglePin} />

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
