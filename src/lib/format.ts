export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

export function formatCost(usd: number): string {
  if (usd >= 1) return `$${usd.toFixed(2)}`;
  if (usd >= 0.01) return `$${usd.toFixed(3)}`;
  return `$${usd.toFixed(4)}`;
}

export function formatDuration(startIso: string | null): string {
  if (!startIso) return "--:--";
  const diff = Date.now() - new Date(startIso).getTime();
  if (diff < 0) return "00:00";
  const h = Math.floor(diff / 3_600_000);
  const m = Math.floor((diff % 3_600_000) / 60_000);
  const s = Math.floor((diff % 60_000) / 1_000);
  if (h > 0) return `${h}h ${String(m).padStart(2, "0")}m`;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export function formatCountdown(resetIso: string | null): string {
  if (!resetIso) return "--:--:--";
  const diff = new Date(resetIso).getTime() - Date.now();
  if (diff <= 0) return "00:00:00";
  const h = Math.floor(diff / 3_600_000);
  const m = Math.floor((diff % 3_600_000) / 60_000);
  const s = Math.floor((diff % 60_000) / 1_000);
  if (h >= 24) {
    const d = Math.floor(h / 24);
    return `${d}d ${h % 24}h`;
  }
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export function formatBurnRate(tpm: number): string {
  if (tpm >= 1000) return `${(tpm / 1000).toFixed(1)}K/m`;
  if (tpm > 0) return `${Math.round(tpm)}/m`;
  return "—";
}

export function quotaPct(used: number | null, limit: number | null): number {
  if (!limit || !used) return 0;
  return Math.min(100, (used / limit) * 100);
}
