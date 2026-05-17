import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { HudState } from "../lib/types";

export function useProviders() {
  const [state, setState] = useState<HudState | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const data = await invoke<HudState>("get_hud_state");
      setState(data);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen<HudState>("hud-update", (e) => {
      setState(e.payload);
      setError(null);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refresh]);

  return { state, error, refresh };
}
