import { useCallback, useEffect, useState } from "react";

import { api, type SettingKey, type SettingsSnapshot, type SettingValues } from "@/lib/api";

/**
 * The app's settings: read on mount and re-read whenever the window gains
 * focus, so hand edits to settings.json show up (spec §9.4). The backend
 * applies each snapshot it returns (theme), so this only tracks it for the UI.
 */
export function useSettings() {
  const [snapshot, setSnapshot] = useState<SettingsSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);

  const apply = useCallback(
    (result: Awaited<ReturnType<typeof api.getSettings>>) => {
      if (result.ok) {
        setSnapshot(result.value);
        setError(null);
      } else {
        setError(result.error.message);
      }
    },
    [],
  );

  const reload = useCallback(async () => apply(await api.getSettings()), [apply]);

  useEffect(() => {
    void reload();
    const onFocus = () => void reload();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [reload]);

  const change = useCallback(
    async <K extends SettingKey>(key: K, value: SettingValues[K]) =>
      apply(await api.setSetting(key, value)),
    [apply],
  );

  const reset = useCallback(async () => apply(await api.resetSettings()), [apply]);

  return { snapshot, error, change, reset };
}

export type SettingsState = ReturnType<typeof useSettings>;
