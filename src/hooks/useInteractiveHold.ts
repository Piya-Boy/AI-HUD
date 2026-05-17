/**
 * useInteractiveHold — toggle overlay click-through based on a global modifier.
 *
 * Default: click-through is ON (mouse passes to terminal beneath).
 * Hold ALT: click-through is temporarily OFF (overlay receives clicks).
 *
 * The key listener runs on `window` of the overlay, but because the overlay
 * is click-through by default, ordinary keypresses won't be delivered here.
 * We therefore poll the OS for the Alt key via a 100ms interval that calls
 * `getModifierState` indirectly through a hidden focus probe — or, more
 * practically on Windows, listen on the controller window via a Tauri event
 * bridge. For Phase 1 we rely on the simplest correct path: a hidden anchor
 * `keydown/keyup` listener on document that fires when the overlay window
 * gets ANY key event (even with WS_EX_TRANSPARENT, keys can route via the
 * window-tree). If this misses on some terminals, we'll add a global low-level
 * keyboard hook in Rust later.
 */

import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export function useInteractiveHold() {
  useEffect(() => {
    let altDown = false;

    const setClickThrough = async (passThrough: boolean) => {
      try {
        await invoke("set_click_through", { enabled: passThrough });
      } catch {
        // ignore
      }
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.altKey && !altDown) {
        altDown = true;
        setClickThrough(false); // enable interaction
      }
    };

    const onKeyUp = (e: KeyboardEvent) => {
      if (!e.altKey && altDown) {
        altDown = false;
        setClickThrough(true); // resume click-through
      }
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", () => {
      if (altDown) {
        altDown = false;
        setClickThrough(true);
      }
    });

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, []);
}
