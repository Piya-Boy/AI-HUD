import { useState, useEffect } from "react";
import type { HudState } from "../lib/types";
import UpdateToast from "./UpdateToast";

// Session: countdown "Resets in X hr Y min"
function formatSessionReset(epochSec: number | null): string {
  if (!epochSec) return "";
  const diff = epochSec * 1000 - Date.now();
  if (diff <= 0) return "Resetting...";
  const totalMins = Math.floor(diff / 60_000);
  const hrs = Math.floor(totalMins / 60);
  const mins = totalMins % 60;
  if (hrs > 0) return `Resets in ${hrs} hr ${mins} min`;
  return `Resets in ${mins} min`;
}

// Weekly: absolute day+time "Resets Mon 11:59 PM"
function formatWeeklyReset(epochSec: number | null): string {
  if (!epochSec) return "";
  const d = new Date(epochSec * 1000);
  if (isNaN(d.getTime()) || d.getTime() <= Date.now()) return "Resetting...";
  const day = d.toLocaleDateString("en-US", { weekday: "short" });
  const time = d.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", hour12: true });
  return `Resets ${day} ${time}`;
}

function useSessionResetLabel(epochSec: number | null): string {
  const [label, setLabel] = useState(() => formatSessionReset(epochSec));
  useEffect(() => {
    if (!epochSec) return;
    setLabel(formatSessionReset(epochSec));
    const id = setInterval(() => setLabel(formatSessionReset(epochSec)), 30_000);
    return () => clearInterval(id);
  }, [epochSec]);
  return label;
}

interface Props {
  state: HudState;
  onMinimize: () => void;
  onRefresh: () => void;
}

const SESSION_BAR_CAP = 200_000;
const WEEKLY_BAR_CAP = 50_000_000;

export default function ExpandedView({ state, onMinimize, onRefresh }: Props) {
  const claude = state.providers.find((p) => p.id === "claude") ?? state.providers[0] ?? null;

  const sessionPct = claude?.session_quota_pct != null
    ? Math.min(100, claude.session_quota_pct)
    : Math.min(100, ((claude?.session_tokens ?? 0) / SESSION_BAR_CAP) * 100);

  const weeklyPct = claude?.weekly_quota_pct != null
    ? Math.min(100, claude.weekly_quota_pct)
    : Math.min(100, ((claude?.weekly_tokens ?? 0) / WEEKLY_BAR_CAP) * 100);

  const sessionLabel = claude?.session_quota_pct != null
    ? `${Math.round(claude.session_quota_pct)}%`
    : "—";

  const weeklyLabel = claude?.weekly_quota_pct != null
    ? `${Math.round(claude.weekly_quota_pct)}%`
    : "—";

  const sessionResetLabel = useSessionResetLabel(claude?.session_reset_epoch ?? null);
  const weeklyResetLabel = formatWeeklyReset(claude?.weekly_reset_epoch ?? null);

  const activityText = state.providers.find((p) => p.is_running)?.activity ?? null;
  const isActive = activityText && activityText !== "Idle";

  return (
    <div
      className="chen-root"
      onDoubleClick={onMinimize}
    >
      {/* Header */}
      <div className="chen-header">
        {/* Pixel alien SVG */}
        <svg viewBox="0 0 11 8" className="chen-alien" fill="#fa6742">
          <rect x="2" y="0" width="1" height="1"/>
          <rect x="8" y="0" width="1" height="1"/>
          <rect x="3" y="1" width="1" height="1"/>
          <rect x="7" y="1" width="1" height="1"/>
          <rect x="2" y="2" width="7" height="1"/>
          <rect x="0" y="3" width="2" height="1"/>
          <rect x="3" y="3" width="1" height="1"/>
          <rect x="7" y="3" width="1" height="1"/>
          <rect x="9" y="3" width="2" height="1"/>
          <rect x="0" y="4" width="11" height="1"/>
          <rect x="0" y="5" width="1" height="1"/>
          <rect x="2" y="5" width="7" height="1"/>
          <rect x="10" y="5" width="1" height="1"/>
          <rect x="0" y="6" width="1" height="1"/>
          <rect x="2" y="6" width="1" height="1"/>
          <rect x="8" y="6" width="1" height="1"/>
          <rect x="10" y="6" width="1" height="1"/>
          <rect x="3" y="7" width="2" height="1"/>
          <rect x="6" y="7" width="2" height="1"/>
        </svg>
        <h1 className="chen-title">Usage</h1>
      </div>

      {/* Session card */}
      <div className="chen-card">
        <div className="chen-card-top">
          <span className="chen-pct">{sessionLabel}</span>
          <span className="chen-badge chen-badge-current">Current</span>
        </div>
        <div className="chen-bar-track">
          <div className="chen-bar-fill chen-bar-orange" style={{ width: `${sessionPct}%` }} />
        </div>
        <div className="chen-reset-text">{sessionResetLabel}</div>
      </div>

      {/* Weekly card */}
      <div className="chen-card">
        <div className="chen-card-top">
          <span className="chen-pct">{weeklyLabel}</span>
          <span className="chen-badge chen-badge-weekly">Weekly</span>
        </div>
        <div className="chen-bar-track">
          <div className="chen-bar-fill chen-bar-green" style={{ width: `${weeklyPct}%` }} />
        </div>
        <div className="chen-reset-text">{weeklyResetLabel}</div>
      </div>

      {/* Update toast — only shown when an update is available */}
      <UpdateToast />

      {/* Footer */}
      <div
        className="chen-footer"
        onMouseDown={(e) => e.stopPropagation()}
        onClick={onRefresh}
      >
        <span className="chen-footer-star">*</span>
        <span className={isActive ? "chen-footer-active" : "chen-footer-idle"}>
          {isActive ? activityText : "Idle"}
        </span>
      </div>
    </div>
  );
}
