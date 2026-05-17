use crate::models::{ActivityStatus, ProviderStatus};
use crate::providers::AiProvider;

pub struct ContinueProvider;

impl ContinueProvider {
    pub fn new() -> Self { Self }
}

impl AiProvider for ContinueProvider {
    fn id(&self) -> &str { "continue" }
    fn display_name(&self) -> &str { "Continue" }

    fn process_names(&self) -> Vec<&str> {
        vec!["continue.exe", "continue"]
    }

    fn collect(&self) -> ProviderStatus {
        let running = self.is_running();
        ProviderStatus {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            model: Some("auto".to_string()),
            is_running: running,
            activity: if running { ActivityStatus::Coding } else { ActivityStatus::Idle },
            last_updated: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        }
    }
}
