import { useCallback, useEffect, useRef, useState } from "react";

import { api } from "@/lib/api";

/**
 * Background transcript scans (spec §5.1). The backend scans on startup and
 * on `refresh()`; this tracks whether one is running (for the spinner) and
 * calls `onFinished` when one ends, so the UI reloads the stored records.
 */
export function useScan(onFinished: () => void) {
  const [scanning, setScanning] = useState(false);
  const finished = useRef(onFinished);
  finished.current = onFinished;

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void (async () => {
      // Subscribe before asking, so a scan that ends in between is not missed.
      const stop = await api.onScanState((running) => {
        if (!active) return;
        setScanning(running);
        if (!running) finished.current();
      });
      if (!active) {
        stop();
        return;
      }
      unlisten = stop;
      if (await api.scanRunning()) {
        if (active) setScanning(true);
      }
    })();
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const refresh = useCallback(() => {
    void api.startScan();
  }, []);

  return { scanning, refresh };
}
