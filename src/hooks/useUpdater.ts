import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface UpdateStatus {
  current_version: string;
  latest_version: string | null;
  update_available: boolean;
  checking: boolean;
  error: string | null;
  channel: string;
}

export function useUpdater() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    const unlisten = listen<UpdateStatus>("update-status", (e) => {
      setStatus(e.payload);
      setDismissed(false);
    });
    return () => { unlisten.then((f) => f()); };
  }, []);

  const checkNow = useCallback(async () => {
    setStatus((prev) => prev ? { ...prev, checking: true } : null);
    try {
      const result = await invoke<UpdateStatus>("check_for_update");
      setStatus(result);
      setDismissed(false);
    } catch {
      // silently ignore — backend returns error inside the status struct
    }
  }, []);

  const skipVersion = useCallback(async (version: string) => {
    await invoke("skip_version", { version });
    setDismissed(true);
  }, []);

  const dismiss = useCallback(() => setDismissed(true), []);

  const showBanner = status?.update_available && !dismissed;

  return { status, showBanner, checkNow, skipVersion, dismiss };
}
