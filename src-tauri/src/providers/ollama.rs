use crate::models::{ActivityStatus, ProviderStatus};
use crate::providers::AiProvider;

pub struct OllamaProvider;

impl OllamaProvider {
    pub fn new() -> Self { Self }
}

impl AiProvider for OllamaProvider {
    fn id(&self) -> &str { "ollama" }
    fn display_name(&self) -> &str { "Ollama" }

    fn process_names(&self) -> Vec<&str> {
        vec!["ollama.exe", "ollama", "ollama_llama_server.exe", "ollama_llama_server"]
    }

    fn collect(&self) -> ProviderStatus {
        let running = self.is_running();
        let mut status = ProviderStatus {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            is_running: running,
            activity: if running { ActivityStatus::Generating } else { ActivityStatus::Idle },
            last_updated: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        };

        if running {
            // Try to query Ollama API at localhost:11434
            if let Some(model_name) = Self::query_active_model() {
                status.model = Some(model_name);
            } else {
                status.model = Some("local model".to_string());
            }
        }

        // Ollama is local/free
        status.session_cost_usd = 0.0;
        status.weekly_cost_usd = 0.0;
        status
    }
}

impl OllamaProvider {
    fn query_active_model() -> Option<String> {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::time::Duration;

        let addr = "127.0.0.1:11434".parse().ok()?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
        stream.set_read_timeout(Some(Duration::from_millis(500))).ok()?;

        let req = "GET /api/ps HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
        stream.write_all(req.as_bytes()).ok()?;

        let mut resp = String::new();
        stream.read_to_string(&mut resp).ok();

        let body = resp.split("\r\n\r\n").nth(1)?;
        let val: serde_json::Value = serde_json::from_str(body).ok()?;
        let models = val.get("models")?.as_array()?;
        if let Some(first) = models.first() {
            return first.get("name")?.as_str().map(|s| s.to_string());
        }
        None
    }
}
