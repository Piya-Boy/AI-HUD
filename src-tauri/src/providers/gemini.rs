use crate::models::{ActivityStatus, ProviderStatus};
use crate::providers::AiProvider;
use std::fs;
use std::path::PathBuf;

pub struct GeminiProvider;

impl GeminiProvider {
    pub fn new() -> Self {
        Self
    }

    fn get_log_path() -> Option<PathBuf> {
        // Gemini CLI stores logs in ~/.gemini/
        let home = dirs_next::home_dir()?;

        // Check common log locations
        let candidates = vec![
            home.join(".gemini").join("logs"),
            home.join(".gemini").join("usage.json"),
            home.join(".config").join("gemini").join("logs"),
        ];

        for path in candidates {
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    fn parse_logs(path: &PathBuf) -> (u64, f64, u64) {
        let mut tokens = 0u64;
        let mut cost = 0.0f64;
        let mut requests = 0u64;

        if path.is_file() {
            if let Ok(content) = fs::read_to_string(path) {
                // Try JSON format
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(usage) = val.get("total_tokens").and_then(|v| v.as_u64()) {
                        tokens = usage;
                    }
                    if let Some(c) = val.get("total_cost").and_then(|v| v.as_f64()) {
                        cost = c;
                    }
                    if let Some(r) = val.get("request_count").and_then(|v| v.as_u64()) {
                        requests = r;
                    }
                }
            }
        } else if path.is_dir() {
            // Scan directory for log files
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    let fp = entry.path();
                    if fp.extension().and_then(|e| e.to_str()) == Some("jsonl")
                        || fp.extension().and_then(|e| e.to_str()) == Some("json")
                    {
                        if let Ok(content) = fs::read_to_string(&fp) {
                            for line in content.lines() {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                                    if let Some(t) = val.get("total_tokens").and_then(|v| v.as_u64()) {
                                        tokens += t;
                                        requests += 1;
                                    }
                                    if let Some(t) = val.get("usage").and_then(|u| {
                                        let i = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                                        let o = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                                        Some(i + o)
                                    }) {
                                        tokens += t;
                                        requests += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        (tokens, cost, requests)
    }
}

impl AiProvider for GeminiProvider {
    fn id(&self) -> &str { "gemini" }
    fn display_name(&self) -> &str { "Gemini CLI" }

    fn process_names(&self) -> Vec<&str> {
        vec!["gemini.exe", "gemini", "gemini-cli.exe", "gemini-cli"]
    }

    fn collect(&self) -> ProviderStatus {
        let running = self.is_running();
        let mut status = ProviderStatus {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            model: Some("gemini-2.5-pro".to_string()),
            is_running: running,
            activity: if running { ActivityStatus::Thinking } else { ActivityStatus::Idle },
            context_window_max: Some(1_000_000),
            last_updated: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        };

        if let Some(log_path) = Self::get_log_path() {
            let (tokens, cost, requests) = Self::parse_logs(&log_path);
            status.weekly_tokens = tokens;
            status.weekly_cost_usd = cost;
            status.weekly_requests = requests;
            status.session_tokens = tokens;
            status.session_requests = requests;
        }

        status
    }
}
