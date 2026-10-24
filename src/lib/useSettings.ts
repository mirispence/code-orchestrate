import { useCallback, useEffect, useRef, useState } from "react";

import { api, type SettingKey, type SettingsSnapshot, type SettingValues } from "@/lib/api";

/**
 * The app's settings: read on mount and re-read whenever the window gains
 * focus, so hand edits to settings.json show up (spec §9.4). The backend
 * applies each snapshot it returns (theme), so this only tracks it for the UI.
 */
export function useSettings() {
  const [snapshot, setSnapshot] = useState<SettingsSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Requests can overlap (a focus re-read during a save). Number them and
  // drop any response older than one already applied, so a stale read never
  // overwrites a newer save.
  const sent = useRef(0);
  const applied = useRef(0);

  const run = useCallback(
    async (request: () => ReturnType<typeof api.getSettings>) => {
      const id = ++sent.current;
      const result = await request();
      if (id < applied.current) return;
      applied.current = id;
      if (result.ok) {
        setSnapshot(result.value);
        setError(null);
      } else {
        setError(result.error.message);
      }
    },
    [],
  );

  const reload = useCallback(() => run(() => api.getSettings()), [run]);

  useEffect(() => {
    void reload();
    const onFocus = () => void reload();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [reload]);

  const change = useCallback(
    <K extends SettingKey>(key: K, value: SettingValues[K]) => run(() => api.setSetting(key, value)),
    [run],
  );

  const reset = useCallback(() => run(() => api.resetSettings()), [run]);

  return { snapshot, error, change, reset };
}

export type SettingsState = ReturnType<typeof useSettings>;
