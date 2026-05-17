use crate::models::{ActivityStatus, ProviderStatus};
use crate::providers::AiProvider;

/// Custom provider for user-defined local AI CLI tools.
/// Detects processes by user-configured names.
#[allow(dead_code)]
pub struct CustomProvider {
    pub custom_id: String,
    pub custom_name: String,
    pub custom_processes: Vec<String>,
}

impl CustomProvider {
    #[allow(dead_code)]
    pub fn new(id: &str, name: &str, processes: Vec<String>) -> Self {
        Self {
            custom_id: id.to_string(),
            custom_name: name.to_string(),
            custom_processes: processes,
        }
    }
}

impl AiProvider for CustomProvider {
    fn id(&self) -> &str { &self.custom_id }
    fn display_name(&self) -> &str { &self.custom_name }

    fn process_names(&self) -> Vec<&str> {
        self.custom_processes.iter().map(|s| s.as_str()).collect()
    }

    fn collect(&self) -> ProviderStatus {
        let running = self.is_running();
        ProviderStatus {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            model: Some("custom".to_string()),
            is_running: running,
            activity: if running { ActivityStatus::Unknown } else { ActivityStatus::Idle },
            last_updated: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        }
    }
}
