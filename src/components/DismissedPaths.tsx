import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";

/**
 * Settings › Suggestions: the dismissed paths, each with Restore (spec §9.1).
 * A restored path can reappear as a Suggestion; `onRestored` lets the app
 * reload its Suggestions.
 */
export function DismissedPaths({ onRestored }: { onRestored?: () => void }) {
  const [paths, setPaths] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    const result = await api.listDismissedPaths();
    if (result.ok) {
      setPaths(result.value);
      setError(null);
    } else {
      setError(result.error.message);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function restore(path: string) {
    const result = await api.restoreDismissedPath(path);
    if (!result.ok) {
      setError(result.error.message);
      return;
    }
    await load();
    onRestored?.();
  }

  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-sm font-medium">Dismissed</span>
      {error !== null && <p className="text-xs text-destructive">{error}</p>}
      {paths !== null && paths.length === 0 && <p className="text-sm text-muted-foreground">No dismissed paths.</p>}
      {paths !== null && paths.length > 0 && (
        <ul className="flex flex-col divide-y rounded-md border">
          {paths.map((path) => (
            <li key={path} className="flex items-center justify-between gap-3 px-3 py-1.5">
              <span className="min-w-0 truncate font-mono text-sm" title={path}>
                {path}
              </span>
              <Button size="sm" variant="outline" onClick={() => void restore(path)}>
                Restore
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
