import { Inbox, Plus, RefreshCw, Settings, Star } from "lucide-react";
import { useId, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { Workspace, WorkspaceStats } from "@/lib/api";
import { formatWhen } from "@/lib/format";
import { tagKey, tagsInUse } from "@/lib/tags";
import { cn } from "@/lib/utils";
import { filterWorkspaces } from "@/lib/workspaceFilter";
import { sortByLastUsed } from "@/lib/workspaceSort";

type SidebarProps = {
  workspaces: Workspace[];
  /** Stats by Workspace id; drives row text and order. */
  stats: ReadonlyMap<number, WorkspaceStats>;
  selectedId: number | null;
  onSelectWorkspace: (id: number) => void;
  onAddWorkspace: () => void;
  onOpenSuggestions: () => void;
  onOpenSettings: () => void;
  settingsOpen: boolean;
  suggestionCount: number;
  suggestionsOpen: boolean;
  /** A transcript scan is running (spinner on the refresh button). */
  scanning: boolean;
  onRefresh: () => void;
};

export function Sidebar({
  workspaces,
  stats,
  selectedId,
  onSelectWorkspace,
  onAddWorkspace,
  onOpenSuggestions,
  onOpenSettings,
  settingsOpen,
  suggestionCount,
  suggestionsOpen,
  scanning,
  onRefresh,
}: SidebarProps) {
  const [query, setQuery] = useState("");
  const [chosenTagKeys, setChosenTagKeys] = useState<string[]>([]);

  const chips = tagsInUse(workspaces);
  // A chosen tag that no Workspace uses any more stops filtering.
  const activeTagKeys = chosenTagKeys.filter((k) => chips.some((t) => tagKey(t) === k));
  const shown = sortByLastUsed(filterWorkspaces(workspaces, query, activeTagKeys), stats);
  // A pinned Workspace appears only under Pinned (spec §8.1).
  const pinned = shown.filter((ws) => ws.pinned);
  const unpinned = shown.filter((ws) => !ws.pinned);

  function toggleTag(key: string) {
    setChosenTagKeys((current) => (current.includes(key) ? current.filter((k) => k !== key) : [...current, key]));
  }

  return (
    <aside className="flex w-72 shrink-0 flex-col gap-3 border-r bg-sidebar p-3 text-sidebar-foreground">
      <div className="flex items-center justify-between gap-2">
        <h1 className="text-base font-semibold">Code Orchestrate</h1>
        <Button size="sm" variant="outline" onClick={onAddWorkspace}>
          <Plus />
          Add Workspace
        </Button>
      </div>
      {workspaces.length > 0 && (
        <Input
          type="search"
          aria-label="Search Workspaces"
          placeholder="Search Workspaces…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      )}
      {chips.length > 0 && (
        <div role="group" aria-label="Filter by tag" className="flex flex-wrap gap-1">
          {chips.map((tag) => {
            const on = activeTagKeys.includes(tagKey(tag));
            return (
              <button
                key={tagKey(tag)}
                type="button"
                aria-pressed={on}
                onClick={() => toggleTag(tagKey(tag))}
                className={cn(
                  "rounded-full px-2 py-0.5 text-xs",
                  on ? "bg-brand text-brand-foreground" : "bg-muted text-muted-foreground hover:bg-sidebar-accent",
                )}
              >
                {tag}
              </button>
            );
          })}
        </div>
      )}
      <Button
        variant="ghost"
        className={cn("justify-start", suggestionsOpen && "bg-sidebar-accent")}
        aria-pressed={suggestionsOpen}
        aria-label={suggestionCount > 0 ? `Suggestions (${suggestionCount})` : "Suggestions"}
        onClick={onOpenSuggestions}
      >
        <Inbox />
        Suggestions
        {suggestionCount > 0 && (
          <span className="ml-auto rounded-full bg-brand px-1.5 text-xs font-medium text-brand-foreground">
            {suggestionCount}
          </span>
        )}
      </Button>
      {workspaces.length > 0 && shown.length === 0 && (
        <p className="px-2 text-sm text-muted-foreground">No matching Workspaces</p>
      )}
      {pinned.length > 0 && (
        <WorkspaceList label="Pinned" workspaces={pinned} stats={stats} selectedId={selectedId} onSelect={onSelectWorkspace} />
      )}
      {unpinned.length > 0 && (
        <WorkspaceList label="All" workspaces={unpinned} stats={stats} selectedId={selectedId} onSelect={onSelectWorkspace} />
      )}
      <div className="mt-auto flex items-center justify-end gap-1 pt-2">
        <Button
          size="icon"
          variant="ghost"
          aria-label="Refresh"
          aria-busy={scanning}
          title={scanning ? "Scanning Sessions…" : "Scan for new Sessions"}
          onClick={onRefresh}
        >
          <RefreshCw className={cn(scanning && "animate-spin")} />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          aria-label="Settings"
          aria-pressed={settingsOpen}
          title="Settings"
          onClick={onOpenSettings}
        >
          <Settings />
        </Button>
      </div>
    </aside>
  );
}

function WorkspaceList({
  label,
  workspaces,
  stats,
  selectedId,
  onSelect,
}: {
  label: string;
  workspaces: Workspace[];
  stats: ReadonlyMap<number, WorkspaceStats>;
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
            <WorkspaceRow
              workspace={ws}
              stats={stats.get(ws.id)}
              selected={ws.id === selectedId}
              onSelect={() => onSelect(ws.id)}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}

/** "<last used> · N Sessions" (spec §8.1). */
function rowStats(stats: WorkspaceStats | undefined): string {
  const count = stats?.sessionCount ?? 0;
  if (count === 0) return "No Sessions";
  return `${formatWhen(stats?.lastUsed ?? null)} · ${count} ${count === 1 ? "Session" : "Sessions"}`;
}

function WorkspaceRow({
  workspace: ws,
  stats,
  selected,
  onSelect,
}: {
  workspace: Workspace;
  stats: WorkspaceStats | undefined;
  selected: boolean;
  onSelect: () => void;
}) {
  const nameId = useId();
  const statsId = useId();
  return (
    <button
      type="button"
      aria-current={selected ? "true" : undefined}
      aria-labelledby={nameId}
      aria-describedby={statsId}
      data-workspace-name={ws.name}
      title={ws.path}
      onClick={onSelect}
      className={cn(
        "flex w-full flex-col rounded-md px-2 py-1.5 text-left text-sm hover:bg-sidebar-accent",
        selected && "bg-sidebar-accent font-medium",
      )}
    >
      <span className="flex w-full items-center gap-1.5">
        <span id={nameId} className="truncate">
          {ws.name}
        </span>
        {ws.pinned && <Star aria-label="Pinned" className="size-3.5 shrink-0 fill-brand text-brand" />}
      </span>
      <span id={statsId} className="truncate text-xs font-normal text-muted-foreground">
        {rowStats(stats)}
      </span>
    </button>
  );
}
