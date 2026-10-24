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
    // Set once any event arrives: events are newer than the status query below.
    let heard = false;
    void (async () => {
      // Subscribe before asking, so a scan that ends in between is not missed.
      const stop = await api.onScanState((running) => {
        if (!active) return;
        heard = true;
        setScanning(running);
        if (!running) finished.current();
      });
      if (!active) {
        stop();
        return;
      }
      unlisten = stop;
      const running = await api.scanRunning();
      if (!active || heard) return;
      // A scan that finished before we listened went unheard, so reload anyway.
      if (running) setScanning(true);
      else finished.current();
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
