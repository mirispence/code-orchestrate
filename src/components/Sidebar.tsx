import { Inbox, Plus } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { Workspace } from "@/lib/api";
import { cn } from "@/lib/utils";

type SidebarProps = {
  workspaces: Workspace[];
  selectedId: number | null;
  onSelectWorkspace: (id: number) => void;
  onAddWorkspace: () => void;
  onOpenSuggestions: () => void;
};

export function Sidebar({
  workspaces,
  selectedId,
  onSelectWorkspace,
  onAddWorkspace,
  onOpenSuggestions,
}: SidebarProps) {
  return (
    <aside className="flex w-72 shrink-0 flex-col gap-3 border-r bg-sidebar p-3 text-sidebar-foreground">
      <div className="flex items-center justify-between gap-2">
        <h1 className="text-base font-semibold">Code Orchestrate</h1>
        <Button size="sm" variant="outline" onClick={onAddWorkspace}>
          <Plus />
          Add Workspace
        </Button>
      </div>
      <Button variant="ghost" className="justify-start" onClick={onOpenSuggestions}>
        <Inbox />
        Suggestions
      </Button>
      {workspaces.length > 0 && (
        <WorkspaceList
          label="All"
          workspaces={workspaces}
          selectedId={selectedId}
          onSelect={onSelectWorkspace}
        />
      )}
    </aside>
  );
}

function WorkspaceList({
  label,
  workspaces,
  selectedId,
  onSelect,
}: {
  label: string;
  workspaces: Workspace[];
  selectedId: number | null;
  onSelect: (id: number) => void;
}) {
  const headingId = `workspace-list-${label.toLowerCase()}`;
  return (
    <section className="flex min-h-0 flex-col gap-1">
      <h2 id={headingId} className="px-2 text-xs font-medium text-muted-foreground uppercase">
        {label}
      </h2>
      <ul aria-labelledby={headingId} className="flex flex-col gap-0.5 overflow-y-auto">
        {workspaces.map((ws) => (
          <li key={ws.id}>
            <button
              type="button"
              aria-current={ws.id === selectedId ? "true" : undefined}
              title={ws.path}
              onClick={() => onSelect(ws.id)}
              className={cn(
                "w-full truncate rounded-md px-2 py-1.5 text-left text-sm hover:bg-sidebar-accent",
                ws.id === selectedId && "bg-sidebar-accent font-medium",
              )}
            >
              {ws.name}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
