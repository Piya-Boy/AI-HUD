use crate::models::{ActivityStatus, ProviderStatus};
use crate::providers::AiProvider;
use std::fs;
use std::path::PathBuf;

pub struct AiderProvider;

impl AiderProvider {
    pub fn new() -> Self {
        Self
    }

    fn get_log_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Some(home) = dirs_next::home_dir() {
            // Aider stores usage in ~/.aider/usage.json or ~/.aider/analytics/
            let usage = home.join(".aider").join("usage.json");
            if usage.exists() { paths.push(usage); }

            let analytics = home.join(".aider").join("analytics");
            if analytics.is_dir() { paths.push(analytics); }

            // Also check for .aider.usage.report.md in CWD or home
            let report = home.join(".aider.usage.report.md");
            if report.exists() { paths.push(report); }
        }
        paths
    }

    fn parse_usage(paths: &[PathBuf]) -> (u64, f64, u64) {
        let mut tokens = 0u64;
        let mut cost = 0.0f64;
        let mut requests = 0u64;

        for path in paths {
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(path) {
                    // Try JSON format
                    if path.extension().and_then(|e| e.to_str()) == Some("json") {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(t) = val.get("total_tokens").and_then(|v| v.as_u64()) {
                                tokens += t;
                            }
                            if let Some(c) = val.get("total_cost").and_then(|v| v.as_f64()) {
                                cost += c;
                            }
                            // Parse session array if present
                            if let Some(sessions) = val.get("sessions").and_then(|v| v.as_array()) {
                                for session in sessions {
                                    if let Some(t) = session.get("tokens").and_then(|v| v.as_u64()) {
                                        tokens += t;
                                    }
                                    if let Some(c) = session.get("cost").and_then(|v| v.as_f64()) {
                                        cost += c;
                                    }
                                    requests += 1;
                                }
                            }
                        }
                    }
                    // Parse markdown report format
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        for line in content.lines() {
                            if line.contains("tokens:") {
                                if let Some(num) = extract_number(line) {
                                    tokens += num;
                                }
                            }
                            if line.contains("cost:") || line.contains("$") {
                                if let Some(c) = extract_cost(line) {
                                    cost += c;
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

fn extract_number(s: &str) -> Option<u64> {
    s.split_whitespace()
        .filter_map(|w| w.replace(",", "").replace("_", "").parse::<u64>().ok())
        .next()
}

fn extract_cost(s: &str) -> Option<f64> {
    s.split('$')
        .nth(1)
        .and_then(|part| {
            part.split_whitespace()
                .next()
                .and_then(|w| w.parse::<f64>().ok())
        })
}

impl AiProvider for AiderProvider {
    fn id(&self) -> &str { "aider" }
    fn display_name(&self) -> &str { "Aider" }

    fn process_names(&self) -> Vec<&str> {
        vec!["aider.exe", "aider", "python.exe", "python3"]
    }

    fn is_running(&self) -> bool {
        // For aider, we need to check if python is running with aider module
        // First check direct aider binary
        if super::is_process_running(&["aider.exe", "aider"]) {
            return true;
        }

        // Check if python is running aider (via command line check)
        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            let output = Command::new("wmic")
                .args(["process", "where", "name='python.exe'", "get", "commandline", "/format:list"])
                .output();
            if let Ok(out) = output {
                let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
                if text.contains("aider") {
                    return true;
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            use std::process::Command;
            let output = Command::new("pgrep").args(["-fa", "aider"]).output();
            if output.map(|o| o.status.success()).unwrap_or(false) {
                return true;
            }
        }

        false
    }

    fn collect(&self) -> ProviderStatus {
        let running = self.is_running();
        let mut status = ProviderStatus {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            model: Some("gpt-4o / claude".to_string()),
            is_running: running,
            activity: if running { ActivityStatus::Coding } else { ActivityStatus::Idle },
            last_updated: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        };

        let paths = Self::get_log_paths();
        let (tokens, cost, requests) = Self::parse_usage(&paths);
        status.session_tokens = tokens;
        status.session_cost_usd = cost;
        status.session_requests = requests;
        status.weekly_tokens = tokens;
        status.weekly_cost_usd = cost;

        status
    }
}
