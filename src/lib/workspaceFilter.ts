import type { Workspace } from "@/lib/api";
import { tagKey } from "@/lib/tags";

/**
 * The sidebar's narrowing (spec §8.1): `query` matches name, path or note
 * case-insensitively, and the Workspace must carry every tag in `tagKeys`.
 */
export function filterWorkspaces(workspaces: Workspace[], query: string, tagKeys: string[]): Workspace[] {
  const q = query.trim().toLowerCase();
  return workspaces.filter((ws) => {
    const keys = new Set(ws.tags.map(tagKey));
    if (!tagKeys.every((k) => keys.has(k))) return false;
    if (q === "") return true;
    return [ws.name, ws.path, ws.note ?? ""].some((field) => field.toLowerCase().includes(q));
  });
}
