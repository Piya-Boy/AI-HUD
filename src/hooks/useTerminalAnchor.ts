import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PhysicalPosition } from "@tauri-apps/api/dpi";
import type { TerminalBounds } from "../lib/types";

export function useTerminalAnchor() {
  const win = getCurrentWindow();
  const isHidden = useRef(true);

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    async function init() {
      try {
        const initial = await invoke<TerminalBounds>("get_terminal_bounds");
        await applyBounds(initial);
      } catch {
        // ignore if no terminal found yet
      }

      unlisten = await listen<TerminalBounds>("terminal-bounds", (event) => {
        applyBounds(event.payload);
      });
    }

    async function applyBounds(b: TerminalBounds) {
      if (!b.is_found || b.width === 0 || b.is_minimized) {
        if (!isHidden.current) {
          await win.hide();
          isHidden.current = true;
        }
        return;
      }

      const size = await win.outerSize();
      const overlayW = size.width;
      // Compensate for Windows invisible 8px DWM shadow border included
      // in GetWindowRect output
      const scale = b.scale_factor || 1;
      const shadow = Math.round(8 * scale);
      const x = Math.round(b.x + b.width - shadow - overlayW);
      const y = Math.round(b.y + shadow);

      await win.setPosition(new PhysicalPosition(x, y));

      if (isHidden.current) {
        await win.show();
        isHidden.current = false;
      }
    }

    init();

    return () => {
      unlisten?.();
    };
  }, []);
}
