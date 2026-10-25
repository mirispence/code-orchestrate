import type { Workspace, WorkspaceStats } from "@/lib/api";

/**
 * Sidebar order (spec §8.1): newest Last used first; Workspaces with no
 * Sessions go last, alphabetically.
 */
export function sortByLastUsed(
  workspaces: Workspace[],
  stats: ReadonlyMap<number, WorkspaceStats>,
): Workspace[] {
  const lastUsed = (ws: Workspace) => {
    const s = stats.get(ws.id);
    return s && s.sessionCount > 0 ? (s.lastUsed ?? 0) : null;
  };
  const byName = (a: Workspace, b: Workspace) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) || a.id - b.id;
  return [...workspaces].sort((a, b) => {
    const la = lastUsed(a);
    const lb = lastUsed(b);
    if (la === null || lb === null) {
      if (la !== lb) return la === null ? 1 : -1;
      return byName(a, b);
    }
    return lb - la || byName(a, b);
  });
}
