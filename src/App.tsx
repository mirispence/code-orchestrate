import { useCallback, useEffect, useState } from "react";

import { AddWorkspaceDialog } from "@/components/AddWorkspaceDialog";
import { EmptyState } from "@/components/EmptyState";
import { SettingsPage } from "@/components/SettingsPage";
import { Sidebar } from "@/components/Sidebar";
import { WorkspaceDetail } from "@/components/WorkspaceDetail";
import { Button } from "@/components/ui/button";
import { api, type Workspace } from "@/lib/api";
import { useSettings } from "@/lib/useSettings";

// Suggestions are wired up by a later ticket.
const notYet = () => {};

export default function App() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [pickedPath, setPickedPath] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const settings = useSettings();
  const closeSettings = useCallback(() => setSettingsOpen(false), []);

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

  function selectWorkspace(id: number) {
    setSettingsOpen(false);
    setSelectedId(id);
  }

  function showWorkspace(id: number) {
    setPickedPath(null);
    selectWorkspace(id);
  }

  const selected = workspaces.find((ws) => ws.id === selectedId) ?? null;

  return (
    <div className="flex h-full">
      <Sidebar
        workspaces={workspaces}
        selectedId={selectedId}
        onSelectWorkspace={selectWorkspace}
        onAddWorkspace={addWorkspace}
        onOpenSuggestions={notYet}
        onOpenSettings={() => setSettingsOpen(true)}
        settingsOpen={settingsOpen}
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
        {settingsOpen ? (
          <SettingsPage settings={settings} onClose={closeSettings} />
        ) : selected ? (
          <WorkspaceDetail workspace={selected} />
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
    </div>
  );
}
