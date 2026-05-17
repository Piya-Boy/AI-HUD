pub mod claude;
pub mod gemini;
pub mod aider;
pub mod ollama;
pub mod openai;
pub mod openrouter;
pub mod continue_cli;
pub mod cursor;
pub mod custom;

use crate::models::ProviderStatus;

/// Trait that all AI CLI providers must implement
pub trait AiProvider: Send + Sync {
    /// Unique provider ID (e.g. "claude")
    fn id(&self) -> &str;

    /// Human-readable name
    fn display_name(&self) -> &str;

    /// Process names to search for (e.g. ["claude", "claude.exe"])
    fn process_names(&self) -> Vec<&str>;

    /// Check if the provider's CLI is currently running
    fn is_running(&self) -> bool {
        let names = self.process_names();
        is_process_running(&names)
    }

    /// Collect current usage/status data
    fn collect(&self) -> ProviderStatus;
}

/// Check if any of the given process names are currently running
pub fn is_process_running(names: &[&str]) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        for name in names {
            let filter = format!("IMAGENAME eq {}", name);
            let output = Command::new("tasklist")
                .args(["/FI", &filter, "/NH", "/FO", "CSV"])
                .output();
            if let Ok(out) = output {
                let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
                if text.contains(&name.to_lowercase()) {
                    return true;
                }
            }
        }
        false
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::process::Command;
        for name in names {
            // Strip .exe suffix for unix
            let clean = name.strip_suffix(".exe").unwrap_or(name);
            let output = Command::new("pgrep").args(["-f", clean]).output();
            if output.map(|o| o.status.success()).unwrap_or(false) {
                return true;
            }
        }
        false
    }
}

/// Manager that holds all registered providers and collects their data
pub struct ProviderManager {
    providers: Vec<Box<dyn AiProvider>>,
}

impl ProviderManager {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn register(&mut self, provider: Box<dyn AiProvider>) {
        self.providers.push(provider);
    }

    /// Register all built-in providers
    pub fn register_defaults(&mut self) {
        self.register(Box::new(claude::ClaudeProvider::new()));
        self.register(Box::new(gemini::GeminiProvider::new()));
        self.register(Box::new(aider::AiderProvider::new()));
        self.register(Box::new(ollama::OllamaProvider::new()));
        self.register(Box::new(openai::OpenAiProvider::new()));
        self.register(Box::new(openrouter::OpenRouterProvider::new()));
        self.register(Box::new(continue_cli::ContinueProvider::new()));
        self.register(Box::new(cursor::CursorProvider::new()));
    }

    /// Collect status from all providers
    pub fn collect_all(&self) -> Vec<ProviderStatus> {
        self.providers
            .iter()
            .map(|p| p.collect())
            .collect()
    }

    /// Collect status only from providers whose processes are detected
    #[allow(dead_code)]
    pub fn collect_active(&self) -> Vec<ProviderStatus> {
        self.providers
            .iter()
            .map(|p| p.collect())
            .collect()
    }
}
