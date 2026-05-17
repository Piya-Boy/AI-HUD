/**
 * useAutoLaunch — listens for ai-session-started / ai-session-stopped events
 * from the Rust process watcher and controls overlay visibility.
 *
 * Position: pins the overlay to the top-right corner of the host terminal
 * window, using the overlay's actual physical size queried at runtime so
 * DPI scaling and any drift in OVERLAY_WIDTH constants can't break alignment.
 */

import { useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PhysicalPosition } from "@tauri-apps/api/dpi";
import type { AiSessionEvent, TerminalBounds } from "../lib/types";

export function useAutoLaunch() {
  const win = getCurrentWindow();
  const activeSessions = useRef<Set<string>>(new Set());
  const isVisible = useRef(false);

  const anchorToTerminal = useCallback(
    async (b: TerminalBounds) => {
      // Query overlay's actual physical size
      const size = await win.outerSize();
      const overlayW = size.width;

      // Windows draws an invisible 8px shadow/resize border around top-level
      // windows (DWM frame). GetWindowRect includes it, so the visible right
      // edge is rect.right - 8 and the visible top edge is rect.top + 8.
      const scale = b.scale_factor || 1;
      const shadow = Math.round(8 * scale);
      const visibleRight = b.x + b.width - shadow;
      const visibleTop = b.y + shadow;

      const x = Math.round(visibleRight - overlayW);
      const y = Math.round(visibleTop);

      console.log("[AutoLaunch] anchor", { bounds: b, overlayW, shadow, x, y });
      await win.setPosition(new PhysicalPosition(x, y));
    },
    [win]
  );

  const showForTerminal = useCallback(
    async (terminalPid: number) => {
      try {
        // Prefer the PID-specific lookup (handles multi-terminal case)
        let bounds = await invoke<TerminalBounds | null>("get_bounds_for_pid", {
          pid: terminalPid,
        });

        // Fallback: Windows Terminal's window often lives under a child/ConPTY
        // process so PID lookup fails — use the generic terminal tracker instead.
        if (!bounds || !bounds.is_found) {
          bounds = await invoke<TerminalBounds>("get_terminal_bounds");
        }

        if (bounds && bounds.is_found && !bounds.is_minimized && bounds.width > 0) {
          if (!isVisible.current) {
            await win.show();
            isVisible.current = true;
          }
          await anchorToTerminal(bounds);
        } else if (!isVisible.current) {
          await win.show();
          isVisible.current = true;
        }
      } catch (e) {
        console.error("[AutoLaunch] showForTerminal error:", e);
        if (!isVisible.current) {
          await win.show();
          isVisible.current = true;
        }
      }
    },
    [win, anchorToTerminal]
  );

  const hideIfEmpty = useCallback(async () => {
    if (activeSessions.current.size === 0 && isVisible.current) {
      await win.hide();
      isVisible.current = false;
    }
  }, [win]);

  useEffect(() => {
    let unlistenStart: (() => void) | null = null;
    let unlistenStop: (() => void) | null = null;
    let unlistenBounds: (() => void) | null = null;

    async function init() {
      try {
        const result = await invoke<{ sessions: AiSessionEvent[] }>("get_active_sessions");
        for (const s of result.sessions) {
          activeSessions.current.add(s.session_id);
        }
        if (activeSessions.current.size > 0) {
          const first = result.sessions.find(s => s.terminal_pid !== 0) ?? result.sessions[0];
          await showForTerminal(first.terminal_pid);
        }
      } catch (e) {
        console.error("[AutoLaunch] init error:", e);
      }

      unlistenStart = await listen<AiSessionEvent>("ai-session-started", async (e) => {
        const ev = e.payload;
        activeSessions.current.add(ev.session_id);
        await showForTerminal(ev.terminal_pid);
      });

      unlistenStop = await listen<AiSessionEvent>("ai-session-stopped", async (e) => {
        const ev = e.payload;
        activeSessions.current.delete(ev.session_id);
        await hideIfEmpty();
      });

      unlistenBounds = await listen<TerminalBounds>("terminal-bounds", async (e) => {
        if (!isVisible.current) return;
        const b = e.payload;
        if (!b.is_found || b.width === 0) return;
        if (b.is_minimized) {
          await win.hide();
          isVisible.current = false;
          return;
        }
        await anchorToTerminal(b);
      });
    }

    init();

    return () => {
      unlistenStart?.();
      unlistenStop?.();
      unlistenBounds?.();
    };
  }, [showForTerminal, hideIfEmpty, anchorToTerminal, win]);
}
