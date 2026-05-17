use crate::models::{ActivityStatus, ProviderStatus};
use crate::providers::AiProvider;

pub struct CursorProvider;

impl CursorProvider {
    pub fn new() -> Self { Self }
}

impl AiProvider for CursorProvider {
    fn id(&self) -> &str { "cursor" }
    fn display_name(&self) -> &str { "Cursor Agent" }

    fn process_names(&self) -> Vec<&str> {
        vec!["cursor.exe", "cursor", "Cursor.exe", "Cursor"]
    }

    fn collect(&self) -> ProviderStatus {
        let running = self.is_running();
        ProviderStatus {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            model: Some("cursor-agent".to_string()),
            is_running: running,
            activity: if running { ActivityStatus::Coding } else { ActivityStatus::Idle },
            last_updated: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        }
    }
}
