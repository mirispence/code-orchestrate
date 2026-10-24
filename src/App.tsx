import { useCallback, useEffect, useState } from "react";

import { AddWorkspaceDialog } from "@/components/AddWorkspaceDialog";
import { EmptyState } from "@/components/EmptyState";
import { Sidebar } from "@/components/Sidebar";
import { WorkspaceDetail } from "@/components/WorkspaceDetail";
import { api, type Workspace } from "@/lib/api";

// Suggestions are wired up by a later ticket.
const notYet = () => {};

export default function App() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [pickedPath, setPickedPath] = useState<string | null>(null);

  const reload = useCallback(async () => {
    setWorkspaces(await api.listWorkspaces());
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function addWorkspace() {
    const path = await api.pickFolder();
    if (path !== null) setPickedPath(path);
  }

  function showWorkspace(id: number) {
    setPickedPath(null);
    setSelectedId(id);
  }

  const selected = workspaces.find((ws) => ws.id === selectedId) ?? null;

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
        {selected ? (
          <WorkspaceDetail workspace={selected} />
        ) : workspaces.length === 0 ? (
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
    </div>
  );
}
