import { useCallback, useEffect, useRef, useState } from "react";

import { AddWorkspaceDialog } from "@/components/AddWorkspaceDialog";
import { AddWorkspaceErrorText } from "@/components/AddWorkspaceErrorText";
import { EditWorkspaceDialog } from "@/components/EditWorkspaceDialog";
import { EmptyState } from "@/components/EmptyState";
import { SettingsPage } from "@/components/SettingsPage";
import { Sidebar } from "@/components/Sidebar";
import { SuggestionsInbox } from "@/components/SuggestionsInbox";
import { WorkspaceDetail } from "@/components/WorkspaceDetail";
import { Button } from "@/components/ui/button";
import { api, type AddWorkspaceError, type Saved, type Suggestion, type Workspace, type WorkspaceStats } from "@/lib/api";
import { tagsInUse } from "@/lib/tags";
import { GitCommandsContext, useGitCommands } from "@/lib/useGitCommands";
import { useScan } from "@/lib/useScan";
import { useSettings } from "@/lib/useSettings";

export default function App() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [stats, setStats] = useState<ReadonlyMap<number, WorkspaceStats>>(new Map());
  const [suggestionsOpen, setSuggestionsOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [pickedPath, setPickedPath] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [actionError, setActionError] = useState<AddWorkspaceError | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const settings = useSettings();
  const gitCommands = useGitCommands();
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
  // Reloads can overlap, so a response older than one already applied is dropped.
  const reloadsSent = useRef(0);
  const reloadApplied = useRef(0);
  const reload = useCallback(async () => {
    const id = ++reloadsSent.current;
    try {
      const [ws, sg, st] = await Promise.all([api.listWorkspaces(), api.listSuggestions(), api.workspaceStats()]);
      if (id < reloadApplied.current) return;
      reloadApplied.current = id;
      setWorkspaces(ws);
      setSuggestions(sg);
      setStats(new Map(st.map((s) => [s.workspaceId, s])));
      setLoadError(null);
    } catch (e) {
      if (id < reloadApplied.current) return;
      reloadApplied.current = id;
      setLoadError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const scan = useScan(reload);

  async function addSuggestion(name: string, path: string) {
    const result = await api.addWorkspace(name, path);
    if (result.ok) {
      showMoveNotice(result.value);
      await reload();
    }
    return result;
  }

  /** Taking another Workspace's former path moves it off, with a notice (spec §6.2). */
  function showMoveNotice(saved: Saved) {
    setNotice(
      saved.movedFrom === null
        ? null
        : `${saved.workspace.path} was a former path of ${saved.movedFrom.workspaceName}. It now belongs to ${saved.workspace.name}.`,
    );
  }

  async function locate(ws: Workspace) {
    const path = await api.pickFolder();
    if (path === null) return;
    const result = await api.repointWorkspace(ws.id, path);
    setActionError(result.ok ? null : result.error);
    if (result.ok) showMoveNotice(result.value);
    void reload();
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
    setActionError(result.ok ? null : result.error);
    void reload();
  }

  function selectWorkspace(id: number) {
    setSettingsOpen(false);
    setSuggestionsOpen(false);
    setSelectedId(id);
    // Re-reading checks every Workspace's folder, including this one (spec §6.3).
    void reload();
  }

  /** Jump to a Workspace, closing whatever dialog or error linked to it. */
  function showWorkspace(id: number) {
    setPickedPath(null);
    setEditingId(null);
    setActionError(null);
    selectWorkspace(id);
  }

  const selected = workspaces.find((ws) => ws.id === selectedId) ?? null;
  const editing = workspaces.find((ws) => ws.id === editingId) ?? null;

  return (
    <div className="flex h-full">
      <Sidebar
        workspaces={workspaces}
        stats={stats}
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
      <main className="h-full min-w-0 flex-1 overflow-y-auto">
        {loadError !== null && (
          <div
            role="alert"
            className="mx-9 mt-6 flex items-center justify-between gap-4 rounded-lg border border-destructive/50 bg-destructive/5 p-3 text-sm text-destructive"
          >
            <span>Could not load Workspaces: {loadError}</span>
            <Button size="sm" variant="outline" onClick={() => void reload()}>
              Retry
            </Button>
          </div>
        )}
        {actionError !== null && (
          <p role="alert" className="mx-9 mt-6 rounded-lg border border-destructive/50 bg-destructive/5 p-3 text-sm text-destructive">
            <AddWorkspaceErrorText error={actionError} onShowWorkspace={showWorkspace} />
          </p>
        )}
        {notice !== null && (
          <div
            role="status"
            className="mx-9 mt-6 flex items-center justify-between gap-4 rounded-lg border border-brand/40 bg-brand-soft p-3 text-sm"
          >
            <span className="min-w-0 break-all">{notice}</span>
            <Button size="sm" variant="ghost" onClick={() => setNotice(null)}>
              Dismiss
            </Button>
          </div>
        )}
        {settingsOpen ? (
          <SettingsPage settings={settings} onClose={closeSettings} onRestored={() => void reload()} />
        ) : suggestionsOpen ? (
          <SuggestionsInbox suggestions={suggestions} onAdd={addSuggestion} onDismiss={dismissSuggestion} />
        ) : selected ? (
          <GitCommandsContext.Provider value={gitCommands}>
            <WorkspaceDetail
              workspace={selected}
              stats={stats.get(selected.id) ?? null}
              onEdit={() => setEditingId(selected.id)}
              onTogglePin={() => void togglePin(selected)}
              onOpenSettings={openSettings}
              onLocate={() => void locate(selected)}
              onFoundMissing={() => void reload()}
            />
          </GitCommandsContext.Provider>
        ) : workspaces.length === 0 && loadError === null ? (
          <EmptyState onAddWorkspace={addWorkspace} onOpenSuggestions={openSuggestions} />
        ) : null}
      </main>
      <AddWorkspaceDialog
        path={pickedPath}
        onClose={() => setPickedPath(null)}
        onAdded={(saved) => {
          showWorkspace(saved.workspace.id);
          showMoveNotice(saved);
        }}
        onShowWorkspace={showWorkspace}
      />
      <EditWorkspaceDialog
        workspace={editing}
        tagsInUse={tagsInUse(workspaces)}
        onClose={() => setEditingId(null)}
        onShowWorkspace={showWorkspace}
        onSaved={(saved) => {
          setEditingId(null);
          showMoveNotice(saved);
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
