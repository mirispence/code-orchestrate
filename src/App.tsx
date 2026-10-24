import { useCallback, useEffect, useState } from "react";

import { AddWorkspaceDialog } from "@/components/AddWorkspaceDialog";
import { EditWorkspaceDialog } from "@/components/EditWorkspaceDialog";
import { EmptyState } from "@/components/EmptyState";
import { Sidebar } from "@/components/Sidebar";
import { WorkspaceDetail } from "@/components/WorkspaceDetail";
import { Button } from "@/components/ui/button";
import { api, type Workspace } from "@/lib/api";
import { tagsInUse } from "@/lib/tags";

// Suggestions are wired up by a later ticket.
const notYet = () => {};

export default function App() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [pickedPath, setPickedPath] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setWorkspaces(await api.listWorkspaces());
      setLoadError(null);
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function addWorkspace() {
    const path = await api.pickFolder();
    if (path !== null) setPickedPath(path);
  }

  async function togglePin(ws: Workspace) {
    const result = await api.setPinned(ws.id, !ws.pinned);
    setActionError(result.ok ? null : result.error.message);
    void reload();
  }

  function showWorkspace(id: number) {
    setPickedPath(null);
    setSelectedId(id);
  }

  const selected = workspaces.find((ws) => ws.id === selectedId) ?? null;
  const editing = workspaces.find((ws) => ws.id === editingId) ?? null;

  return (
    <div className="flex h-full">
      <Sidebar
        workspaces={workspaces}
        selectedId={selectedId}
        onSelectWorkspace={setSelectedId}
        onAddWorkspace={addWorkspace}
        onOpenSuggestions={notYet}
      />
      <main className="min-w-0 flex-1">
        {loadError !== null && (
          <div
            role="alert"
            className="m-4 flex items-center justify-between gap-4 rounded-md border border-destructive/50 p-3 text-sm text-destructive"
          >
            <span>Could not load Workspaces: {loadError}</span>
            <Button size="sm" variant="outline" onClick={() => void reload()}>
              Retry
            </Button>
          </div>
        )}
        {actionError !== null && (
          <p role="alert" className="m-4 rounded-md border border-destructive/50 p-3 text-sm text-destructive">
            {actionError}
          </p>
        )}
        {selected ? (
          <WorkspaceDetail
            workspace={selected}
            onEdit={() => setEditingId(selected.id)}
            onTogglePin={() => void togglePin(selected)}
          />
        ) : workspaces.length === 0 && loadError === null ? (
          <EmptyState onAddWorkspace={addWorkspace} onOpenSuggestions={notYet} />
        ) : null}
      </main>
      <AddWorkspaceDialog
        path={pickedPath}
        onClose={() => setPickedPath(null)}
        onAdded={(ws) => {
          showWorkspace(ws.id);
          void reload();
        }}
        onShowWorkspace={showWorkspace}
      />
      <EditWorkspaceDialog
        workspace={editing}
        tagsInUse={tagsInUse(workspaces)}
        onClose={() => setEditingId(null)}
        onSaved={() => {
          setEditingId(null);
          void reload();
        }}
        onRemoved={(id) => {
          setEditingId(null);
          if (selectedId === id) setSelectedId(null);
          void reload();
        }}
      />
    </div>
  );
}
