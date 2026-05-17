use serde::{Deserialize, Serialize};

/// Represents the activity status of an AI CLI tool
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ActivityStatus {
    Idle,
    Thinking,
    Coding,
    Generating,
    Refactoring,
    Musing,
    Analyzing,
    Searching,
    Unknown,
}

impl Default for ActivityStatus {
    fn default() -> Self {
        ActivityStatus::Idle
    }
}

impl ActivityStatus {
    #[allow(dead_code)]
    pub fn label(&self) -> &str {
        match self {
            ActivityStatus::Idle => "Idle",
            ActivityStatus::Thinking => "Thinking...",
            ActivityStatus::Coding => "Coding...",
            ActivityStatus::Generating => "Generating...",
            ActivityStatus::Refactoring => "Refactoring...",
            ActivityStatus::Musing => "Musing...",
            ActivityStatus::Analyzing => "Analyzing...",
            ActivityStatus::Searching => "Searching...",
            ActivityStatus::Unknown => "Working...",
        }
    }
}

/// Per-provider usage and status data sent to the frontend
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderStatus {
    /// Unique provider identifier (e.g. "claude", "gemini", "aider")
    pub id: String,
    /// Human-readable display name
    pub display_name: String,
    /// Currently active model name if detectable
    pub model: Option<String>,
    /// Whether the CLI process is currently running
    pub is_running: bool,
    /// Current activity status
    pub activity: ActivityStatus,

    // Session stats
    pub session_tokens: u64,
    pub session_input_tokens: u64,
    pub session_output_tokens: u64,
    pub session_cache_read: u64,
    pub session_cache_write: u64,
    pub session_cost_usd: f64,
    pub session_requests: u64,
    pub session_start_iso: Option<String>,

    // Daily stats
    pub daily_tokens: u64,
    pub daily_cost_usd: f64,
    pub daily_requests: u64,

    // Weekly stats
    pub weekly_tokens: u64,
    pub weekly_cost_usd: f64,
    pub weekly_requests: u64,

    // Quota info — raw values from API ping
    pub quota_limit: Option<u64>,
    pub quota_used: Option<u64>,
    pub quota_reset_iso: Option<String>,        // kept for compat (unused)
    pub session_reset_iso: Option<String>,      // kept for compat (unused)
    // Reset times as Unix epoch seconds — avoids Thai Buddhist calendar locale bug
    pub session_reset_epoch: Option<u64>,       // 5h window reset (epoch s)
    pub weekly_reset_epoch: Option<u64>,        // 7d window reset (epoch s)
    // API ping percentages (0.0–100.0); None = ping not done or failed
    pub session_quota_pct: Option<f64>,  // 5h window utilization × 100
    pub weekly_quota_pct: Option<f64>,   // 7d window utilization × 100

    // Context window
    pub context_window_max: Option<u64>,
    pub context_window_used: Option<u64>,

    // Burn rate (tokens per minute, calculated over last 5 minutes)
    pub burn_rate_tpm: f64,

    pub last_updated: String,
}

/// The complete state sent to the frontend every tick
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct HudState {
    pub providers: Vec<ProviderStatus>,
    pub any_active: bool,
    pub total_session_tokens: u64,
    pub total_daily_tokens: u64,
    pub total_weekly_tokens: u64,
    pub total_session_cost: f64,
    pub total_daily_cost: f64,
    pub total_weekly_cost: f64,
    pub active_count: u32,
    pub last_updated: String,
}

/// Settings persisted to disk
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HudSettings {
    pub poll_interval_ms: u64,
    pub compact_mode: bool,
    pub click_through: bool,
    pub opacity: f64,
    pub snap_corner: Option<String>,
    pub enabled_providers: Vec<String>,
    pub show_notifications: bool,
    pub quota_warning_pct: f64,
}

impl Default for HudSettings {
    fn default() -> Self {
        Self {
            poll_interval_ms: 1000,
            compact_mode: false,
            click_through: false,
            opacity: 1.0,
            snap_corner: None,
            enabled_providers: vec![
                "claude".into(),
                "gemini".into(),
                "aider".into(),
                "ollama".into(),
                "openai".into(),
                "openrouter".into(),
                "continue".into(),
                "cursor".into(),
            ],
            show_notifications: true,
            quota_warning_pct: 85.0,
        }
    }
}
