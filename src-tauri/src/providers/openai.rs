use crate::models::{ActivityStatus, ProviderStatus};
use crate::providers::AiProvider;
use std::fs;
use std::path::PathBuf;

pub struct OpenAiProvider;

impl OpenAiProvider {
    pub fn new() -> Self { Self }

    fn get_log_path() -> Option<PathBuf> {
        let home = dirs_next::home_dir()?;
        let candidates = vec![
            home.join(".codex").join("usage.json"),
            home.join(".openai").join("usage.json"),
            home.join(".config").join("openai").join("usage.json"),
        ];
        for p in candidates {
            if p.exists() { return Some(p); }
        }
        None
    }
}

impl AiProvider for OpenAiProvider {
    fn id(&self) -> &str { "openai" }
    fn display_name(&self) -> &str { "OpenAI Codex" }

    fn process_names(&self) -> Vec<&str> {
        vec!["codex.exe", "codex", "openai.exe", "openai"]
    }

    fn collect(&self) -> ProviderStatus {
        let running = self.is_running();
        let mut status = ProviderStatus {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            model: Some("codex-1".to_string()),
            is_running: running,
            activity: if running { ActivityStatus::Coding } else { ActivityStatus::Idle },
            context_window_max: Some(200_000),
            last_updated: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        };

        if let Some(path) = Self::get_log_path() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(t) = val.get("total_tokens").and_then(|v| v.as_u64()) {
                        status.weekly_tokens = t;
                        status.session_tokens = t;
                    }
                    if let Some(c) = val.get("total_cost").and_then(|v| v.as_f64()) {
                        status.weekly_cost_usd = c;
                        status.session_cost_usd = c;
                    }
                }
            }
        }
        status
    }
}
