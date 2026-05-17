export interface ProviderStatus {
  id: string;
  display_name: string;
  model: string | null;
  is_running: boolean;
  activity: string;
  session_tokens: number;
  session_input_tokens: number;
  session_output_tokens: number;
  session_cache_read: number;
  session_cache_write: number;
  session_cost_usd: number;
  session_requests: number;
  session_start_iso: string | null;
  daily_tokens: number;
  daily_cost_usd: number;
  daily_requests: number;
  weekly_tokens: number;
  weekly_cost_usd: number;
  weekly_requests: number;
  quota_limit: number | null;
  quota_used: number | null;
  quota_reset_iso: string | null;
  session_reset_iso: string | null;
  session_reset_epoch: number | null;   // 5h window reset (Unix epoch seconds)
  weekly_reset_epoch: number | null;    // 7d window reset (Unix epoch seconds)
  session_quota_pct: number | null;     // 0–100, from API ping 5h window
  weekly_quota_pct: number | null;      // 0–100, from API ping 7d window
  context_window_max: number | null;
  context_window_used: number | null;
  burn_rate_tpm: number;
  last_updated: string;
}

export interface HudState {
  providers: ProviderStatus[];
  any_active: boolean;
  total_session_tokens: number;
  total_daily_tokens: number;
  total_weekly_tokens: number;
  total_session_cost: number;
  total_daily_cost: number;
  total_weekly_cost: number;
  active_count: number;
  last_updated: string;
}

export interface HudSettings {
  poll_interval_ms: number;
  compact_mode: boolean;
  click_through: boolean;
  opacity: number;
  snap_corner: string | null;
  enabled_providers: string[];
  show_notifications: boolean;
  quota_warning_pct: number;
}

/** Emitted by Rust when an AI CLI process starts or stops */
export interface AiSessionEvent {
  session_id: string;
  provider_id: string;
  pid: number;
  terminal_pid: number;
  exe_name: string;
}

export interface TerminalBounds {
  x: number;
  y: number;
  width: number;
  height: number;
  is_minimized: boolean;
  is_found: boolean;
  terminal_name: string;
  scale_factor: number;
}
