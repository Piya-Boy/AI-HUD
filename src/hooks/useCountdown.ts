import { useEffect, useState } from "react";
import { formatCountdown } from "../lib/format";

export function useCountdown(resetIso: string | null): string {
  const [label, setLabel] = useState("--:--:--");

  useEffect(() => {
    if (!resetIso) return;
    const update = () => setLabel(formatCountdown(resetIso));
    update();
    const id = setInterval(update, 1000);
    return () => clearInterval(id);
  }, [resetIso]);

  return label;
}
