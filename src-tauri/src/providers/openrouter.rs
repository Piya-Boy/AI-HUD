use crate::models::{ActivityStatus, ProviderStatus};
use crate::providers::AiProvider;

pub struct OpenRouterProvider;

impl OpenRouterProvider {
    pub fn new() -> Self { Self }
}

impl AiProvider for OpenRouterProvider {
    fn id(&self) -> &str { "openrouter" }
    fn display_name(&self) -> &str { "OpenRouter" }

    fn process_names(&self) -> Vec<&str> {
        vec!["openrouter.exe", "openrouter", "or.exe", "or"]
    }

    fn collect(&self) -> ProviderStatus {
        let running = self.is_running();
        ProviderStatus {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            model: Some("auto".to_string()),
            is_running: running,
            activity: if running { ActivityStatus::Generating } else { ActivityStatus::Idle },
            last_updated: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        }
    }
}
