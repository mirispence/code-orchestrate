// Tag rules shared by the edit dialog and the sidebar (spec §6.2): free-form,
// compared case-insensitively, stored as first typed. The store enforces the
// same rules; these keep the UI consistent before a save.
import type { Workspace } from "@/lib/api";

/** A tag's case-insensitive identity. */
export function tagKey(tag: string): string {
  return tag.trim().toLowerCase();
}

/** Append `raw` unless it is blank or already present in another case. */
export function addTag(tags: string[], raw: string): string[] {
  const key = tagKey(raw);
  if (key === "" || tags.some((t) => tagKey(t) === key)) return tags;
  return [...tags, raw.trim()];
}

/** Every tag some Workspace uses, once each, sorted case-insensitively. */
export function tagsInUse(workspaces: Workspace[]): string[] {
  const byKey = new Map<string, string>();
  for (const ws of workspaces) {
    for (const tag of ws.tags) if (!byKey.has(tagKey(tag))) byKey.set(tagKey(tag), tag);
  }
  return [...byKey.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([, tag]) => tag);
}
