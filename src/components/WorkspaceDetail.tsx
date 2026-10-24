import type { Workspace } from "@/lib/api";

/** Detail pane for the selected Workspace (spec §8.2). Later tickets add actions and stats. */
export function WorkspaceDetail({ workspace }: { workspace: Workspace }) {
  return (
    <div className="flex flex-col gap-1 p-6">
      <h2 className="text-xl font-semibold">{workspace.name}</h2>
      <p className="font-mono text-sm break-all text-muted-foreground">{workspace.path}</p>
    </div>
  );
}
