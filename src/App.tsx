import { useCallback, useEffect, useState } from "react";

import { AddWorkspaceDialog } from "@/components/AddWorkspaceDialog";
import { EditWorkspaceDialog } from "@/components/EditWorkspaceDialog";
import { EmptyState } from "@/components/EmptyState";
import { SettingsPage } from "@/components/SettingsPage";
import { Sidebar } from "@/components/Sidebar";
import { SuggestionsInbox } from "@/components/SuggestionsInbox";
import { WorkspaceDetail } from "@/components/WorkspaceDetail";
import { Button } from "@/components/ui/button";
import { api, type Suggestion, type Workspace } from "@/lib/api";
import { tagsInUse } from "@/lib/tags";
import { useScan } from "@/lib/useScan";
import { useSettings } from "@/lib/useSettings";

export default function App() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [suggestionsOpen, setSuggestionsOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [pickedPath, setPickedPath] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const settings = useSettings();
  const openSettings = useCallback(() => {
    setSuggestionsOpen(false);
    setSettingsOpen(true);
  }, []);
  const closeSettings = useCallback(() => setSettingsOpen(false), []);
  const openSuggestions = useCallback(() => {
    setSettingsOpen(false);
    setSuggestionsOpen(true);
  }, []);

  // Stored records render at once; a finished scan reloads them (spec §5.1).
  const reload = useCallback(async () => {
    try {
      const [ws, sg] = await Promise.all([api.listWorkspaces(), api.listSuggestions()]);
      setWorkspaces(ws);
      setSuggestions(sg);
      setLoadError(null);
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const scan = useScan(reload);

  async function addSuggestion(name: string, path: string) {
    const result = await api.addWorkspace(name, path);
    if (result.ok) await reload();
    return result;
  }

  async function dismissSuggestion(path: string) {
    const result = await api.dismissPath(path);
    if (result.ok) await reload();
    return result;
  }

  async function addWorkspace() {
    const path = await api.pickFolder();
    if (path !== null) setPickedPath(path);
  }

  async function togglePin(ws: Workspace) {
    const result = await api.setPinned(ws.id, !ws.pinned);
    setActionError(result.ok ? null : result.error.message);
    void reload();
  }

  function selectWorkspace(id: number) {
    setSettingsOpen(false);
    setSuggestionsOpen(false);
    setSelectedId(id);
  }

  function showWorkspace(id: number) {
    setPickedPath(null);
    selectWorkspace(id);
  }

  const selected = workspaces.find((ws) => ws.id === selectedId) ?? null;
  const editing = workspaces.find((ws) => ws.id === editingId) ?? null;

  return (
    <div className="flex h-full">
      <Sidebar
        workspaces={workspaces}
        selectedId={selectedId}
        onSelectWorkspace={selectWorkspace}
        onAddWorkspace={addWorkspace}
        onOpenSuggestions={openSuggestions}
        onOpenSettings={openSettings}
        settingsOpen={settingsOpen}
        suggestionCount={suggestions.length}
        suggestionsOpen={suggestionsOpen}
        scanning={scan.scanning}
        onRefresh={scan.refresh}
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
        {settingsOpen ? (
          <SettingsPage settings={settings} onClose={closeSettings} />
        ) : suggestionsOpen ? (
          <SuggestionsInbox suggestions={suggestions} onAdd={addSuggestion} onDismiss={dismissSuggestion} />
        ) : selected ? (
          <WorkspaceDetail
            workspace={selected}
            onEdit={() => setEditingId(selected.id)}
            onTogglePin={() => void togglePin(selected)}
            onOpenSettings={openSettings}
          />
        ) : workspaces.length === 0 && loadError === null ? (
          <EmptyState onAddWorkspace={addWorkspace} onOpenSuggestions={openSuggestions} />
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
