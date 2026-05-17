import { motion } from "framer-motion";
import type { ProviderStatus } from "../lib/types";
import { getProviderMeta, ACTIVITY_LABELS } from "../lib/constants";
import { formatTokens } from "../lib/format";


interface Props {
  provider: ProviderStatus;
  isExpanded: boolean;
  onToggleExpand: () => void;
}

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

// Weekly: absolute day+time in local timezone "Resets Mon 11:59 PM"
function formatWeeklyReset(epochSec: number | null): string {
  if (!epochSec) return "";
  const d = new Date(epochSec * 1000);
  if (isNaN(d.getTime()) || d.getTime() <= Date.now()) return "Resetting...";
  const day = d.toLocaleDateString("en-US", { weekday: "short" });
  const time = d.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", hour12: true });
  return `Resets ${day} ${time}`;
}

export default function ProviderCard({ provider, isExpanded, onToggleExpand }: Props) {
  const meta = getProviderMeta(provider.id);
  const activityLabel = ACTIVITY_LABELS[provider.activity] ?? provider.activity ?? "Idle";
  const isActive = provider.is_running;
  // Session bar (5h window): API ping % is authoritative, fallback to token ratio
  const SESSION_TOKEN_CAP = 200_000;
  const sessionBarPct = provider.session_quota_pct != null
    ? Math.min(100, provider.session_quota_pct)
    : Math.min(100, (provider.session_tokens / SESSION_TOKEN_CAP) * 100);

  // Weekly bar (7d window): API ping % is authoritative, fallback to token ratio
  const WEEKLY_TOKEN_CAP = 50_000_000;
  const weeklyBarPct = provider.weekly_quota_pct != null
    ? Math.min(100, provider.weekly_quota_pct)
    : Math.min(100, (provider.weekly_tokens / WEEKLY_TOKEN_CAP) * 100);

  const sessionBarColor = sessionBarPct > 85 ? "#ef4444" : "#f97316";
  const weeklyBarColor = "#22c55e";

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -8 }}
      transition={{ duration: 0.25, ease: "easeOut" }}
      className="usage-card"
      onClick={(e) => {
        e.stopPropagation();
        onToggleExpand();
      }}
    >
      {/* Session row — shows API ping % when available, else token count */}
      <div className="usage-row">
        <div className="usage-row-left">
          <span className="usage-pct" style={{ color: "rgba(255,255,255,0.92)" }}>
            {provider.session_quota_pct != null
              ? `${Math.round(provider.session_quota_pct)}%`
              : formatTokens(provider.session_tokens)}
          </span>
          <span className="usage-reset-text">
            {formatSessionReset(provider.session_reset_epoch)}
          </span>
        </div>
        <span className="usage-tag usage-tag-current">Session</span>
      </div>
      <div className="usage-bar-track">
        <motion.div
          className="usage-bar-fill"
          initial={{ width: 0 }}
          animate={{ width: `${sessionBarPct}%` }}
          transition={{ duration: 0.7, ease: [0.4, 0, 0.2, 1] }}
          style={{ background: sessionBarColor }}
        />
      </div>

      {/* Weekly row — shows API ping % when available, else token count */}
      <div className="usage-row" style={{ marginTop: 10 }}>
        <div className="usage-row-left">
          <span className="usage-pct" style={{ color: "rgba(255,255,255,0.92)" }}>
            {provider.weekly_quota_pct != null
              ? `${Math.round(provider.weekly_quota_pct)}%`
              : formatTokens(provider.weekly_tokens)}
          </span>
          <span className="usage-reset-text">
            {formatWeeklyReset(provider.weekly_reset_epoch)}
          </span>
        </div>
        <span className="usage-tag usage-tag-weekly">Weekly</span>
      </div>
      <div className="usage-bar-track">
        <motion.div
          className="usage-bar-fill"
          initial={{ width: 0 }}
          animate={{ width: `${weeklyBarPct}%` }}
          transition={{ duration: 0.7, ease: [0.4, 0, 0.2, 1], delay: 0.1 }}
          style={{ background: weeklyBarColor }}
        />
      </div>

      {/* Expanded details */}
      {isExpanded && (
        <motion.div
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          transition={{ duration: 0.2 }}
          className="usage-details"
        >
          <div className="usage-detail-grid">
            <DetailRow label="Model" value={provider.model ?? meta.label} />
            <DetailRow label="Requests" value={String(provider.session_requests)} />
            {provider.context_window_max && (
              <DetailRow
                label="Context"
                value={`${((provider.context_window_used ?? 0) / provider.context_window_max * 100).toFixed(0)}%`}
              />
            )}
            {provider.burn_rate_tpm > 0 && (
              <DetailRow label="Burn rate" value={`${Math.round(provider.burn_rate_tpm)}/min`} />
            )}
          </div>
        </motion.div>
      )}

      {/* Activity status */}
      {isActive && (
        <div className="usage-activity">
          <span className="usage-activity-star">*</span>
          <span className="usage-activity-text">{activityLabel}...</span>
        </div>
      )}
    </motion.div>
  );
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="usage-detail-row">
      <span className="usage-detail-label">{label}</span>
      <span className="usage-detail-value">{value}</span>
    </div>
  );
}
