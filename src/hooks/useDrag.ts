import { useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

export function useDrag() {
  const dragRef = useRef(false);

  const onDragStart = useCallback(async (e: React.MouseEvent) => {
    e.preventDefault();
    dragRef.current = true;
    try {
      await invoke("start_drag");
    } catch {
      // ignore drag errors
    }
  }, []);

  return { onDragStart, dragRef };
}
