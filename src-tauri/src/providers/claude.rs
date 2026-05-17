use crate::models::{ActivityStatus, ProviderStatus};
use crate::providers::AiProvider;

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ── JSONL structures ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct JournalLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    message: Option<JournalMessage>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
}

#[derive(Deserialize)]
struct JournalMessage {
    usage: Option<ApiUsage>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

// ── Pricing (USD per million tokens) ─────────────────────────────────────────

const PRICE_INPUT_PER_M: f64 = 3.0;
const PRICE_OUTPUT_PER_M: f64 = 15.0;
const PRICE_CACHE_WRITE_PER_M: f64 = 3.75;
const PRICE_CACHE_READ_PER_M: f64 = 0.30;

fn calc_cost(input: u64, output: u64, cache_write: u64, cache_read: u64) -> f64 {
    (input as f64 * PRICE_INPUT_PER_M
        + output as f64 * PRICE_OUTPUT_PER_M
        + cache_write as f64 * PRICE_CACHE_WRITE_PER_M
        + cache_read as f64 * PRICE_CACHE_READ_PER_M)
        / 1_000_000.0
}

// ── Per-file stats ────────────────────────────────────────────────────────────

struct FileStats {
    input: u64,
    output: u64,
    cache_write: u64,
    cache_read: u64,
    requests: u32,
    modified: SystemTime,
    last_model: Option<String>,
}

fn parse_jsonl(path: &PathBuf) -> FileStats {
    let mut stats = FileStats {
        input: 0, output: 0,
        cache_write: 0, cache_read: 0,
        requests: 0,
        modified: UNIX_EPOCH,
        last_model: None,
    };

    if let Ok(meta) = path.metadata() {
        if let Ok(m) = meta.modified() { stats.modified = m; }
    }

    let Ok(content) = fs::read_to_string(path) else { return stats };

    let mut seen_requests = std::collections::HashSet::new();

    for line in content.lines() {
        let Ok(entry) = serde_json::from_str::<JournalLine>(line) else { continue };
        if entry.line_type.as_deref() != Some("assistant") { continue; }

        let rid = match &entry.request_id {
            Some(id) => id.clone(),
            None => continue,
        };
        if !seen_requests.insert(rid) { continue; }

        let Some(msg) = entry.message else { continue };
        let Some(u) = msg.usage else { continue };
        if u.input_tokens == 0 && u.output_tokens == 0 { continue; }

        stats.input += u.input_tokens;
        stats.output += u.output_tokens;
        stats.cache_write += u.cache_creation_input_tokens;
        stats.cache_read += u.cache_read_input_tokens;
        stats.requests += 1;

        if let Some(m) = msg.model { stats.last_model = Some(m); }
    }

    stats
}

// ── Aggregation ───────────────────────────────────────────────────────────────

struct Aggregated {
    weekly_input: u64,
    weekly_output: u64,
    weekly_cache_write: u64,
    weekly_cache_read: u64,
    weekly_requests: u64,
    session_input: u64,
    session_output: u64,
    session_cache_write: u64,
    session_cache_read: u64,
    session_requests: u32,
    session_model: Option<String>,
}

impl Aggregated {
    fn session_tokens(&self) -> u64 { self.session_input + self.session_output }
    fn session_cost(&self) -> f64 {
        calc_cost(self.session_input, self.session_output,
                  self.session_cache_write, self.session_cache_read)
    }
    fn weekly_tokens(&self) -> u64 { self.weekly_input + self.weekly_output }
    fn weekly_cost(&self) -> f64 {
        calc_cost(self.weekly_input, self.weekly_output,
                  self.weekly_cache_write, self.weekly_cache_read)
    }
}

fn collect_jsonl_files(dir: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(sub) = fs::read_dir(&path) {
                for sub_entry in sub.flatten() {
                    let sp = sub_entry.path();
                    if sp.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                        out.push(sp);
                    }
                }
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
    out
}

fn collect_stats(projects_dir: &PathBuf) -> Aggregated {
    let week_threshold = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().saturating_sub(7 * 24 * 3600))
        .unwrap_or(0);

    let mut agg = Aggregated {
        weekly_input: 0, weekly_output: 0,
        weekly_cache_write: 0, weekly_cache_read: 0,
        weekly_requests: 0,
        session_input: 0, session_output: 0,
        session_cache_write: 0, session_cache_read: 0,
        session_requests: 0,
        session_model: None,
    };
    let mut latest_modified = UNIX_EPOCH;

    let Ok(projects) = fs::read_dir(projects_dir) else { return agg };

    for project in projects.flatten() {
        let project_path = project.path();
        if !project_path.is_dir() { continue; }

        for fp in collect_jsonl_files(&project_path) {
            let stats = parse_jsonl(&fp);
            let mod_secs = stats.modified
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            if mod_secs >= week_threshold {
                agg.weekly_input += stats.input;
                agg.weekly_output += stats.output;
                agg.weekly_cache_write += stats.cache_write;
                agg.weekly_cache_read += stats.cache_read;
                agg.weekly_requests += stats.requests as u64;
            }

            if stats.modified > latest_modified && stats.requests > 0 {
                latest_modified = stats.modified;
                agg.session_input = stats.input;
                agg.session_output = stats.output;
                agg.session_cache_write = stats.cache_write;
                agg.session_cache_read = stats.cache_read;
                agg.session_requests = stats.requests;
                agg.session_model = stats.last_model;
            }
        }
    }

    agg
}

// ── Credentials ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OAuthEntry>,
}

#[derive(Deserialize)]
struct OAuthEntry {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

fn credentials_path() -> Option<PathBuf> {
    let home = dirs_next::home_dir()?;
    let path = home.join(".claude").join(".credentials.json");
    if path.exists() { Some(path) } else { None }
}

fn read_access_token() -> Option<(String, Option<String>)> {
    let path = credentials_path()?;
    let content = fs::read_to_string(&path).ok()?;
    let creds: CredentialsFile = serde_json::from_str(&content).ok()?;
    let oauth = creds.claude_ai_oauth?;
    let token = oauth.access_token?;
    if token.is_empty() { return None; }
    Some((token, oauth.subscription_type))
}

// ── Quota ping ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct QuotaInfo {
    session_pct: f64,            // 0.0–1.0  (5h window)
    weekly_pct: f64,             // 0.0–1.0  (7d window)
    session_reset_epoch: Option<u64>,   // 5h-reset as Unix epoch seconds
    weekly_reset_epoch: Option<u64>,    // 7d-reset as Unix epoch seconds
}

fn ping_quota(token: &str) -> Option<QuotaInfo> {
    let body = r#"{"model":"claude-haiku-4-5-20251001","max_tokens":1,"messages":[{"role":"user","content":"x"}]}"#;

    let output = std::process::Command::new("curl")
        .args([
            "-s", "-i",
            "--max-time", "15",
            "-X", "POST",
            "https://api.anthropic.com/v1/messages",
            "-H", &format!("Authorization: Bearer {}", token),
            "-H", "anthropic-version: 2023-06-01",
            "-H", "anthropic-beta: oauth-2025-04-20",
            "-H", "User-Agent: claude-code/2.1.0",
            "-H", "Content-Type: application/json",
            "-d", body,
        ])
        .output()
        .ok()?;

    let response = String::from_utf8_lossy(&output.stdout);
    parse_ping_headers(&response)
}

fn header_value<'a>(line: &'a str) -> Option<&'a str> {
    line.splitn(2, ':').nth(1).map(|v| v.trim())
}

fn parse_ping_headers(response: &str) -> Option<QuotaInfo> {
    let mut session_pct: Option<f64> = None;
    let mut weekly_pct: Option<f64> = None;
    let mut session_reset_epoch: Option<u64> = None;
    let mut weekly_reset_epoch: Option<u64> = None;

    for line in response.lines() {
        let lower = line.to_lowercase();

        if lower.starts_with("anthropic-ratelimit-unified-5h-utilization:") {
            session_pct = header_value(line).and_then(|v| v.parse::<f64>().ok());
        } else if lower.starts_with("anthropic-ratelimit-unified-7d-utilization:") {
            weekly_pct = header_value(line).and_then(|v| v.parse::<f64>().ok());
        } else if lower.starts_with("anthropic-ratelimit-unified-5h-reset:") {
            session_reset_epoch = header_value(line).and_then(|v| v.parse::<u64>().ok());
        } else if lower.starts_with("anthropic-ratelimit-unified-7d-reset:") {
            weekly_reset_epoch = header_value(line).and_then(|v| v.parse::<u64>().ok());
        }
    }

    Some(QuotaInfo {
        session_pct: session_pct.unwrap_or(0.0),
        weekly_pct: weekly_pct.unwrap_or(0.0),
        session_reset_epoch,
        weekly_reset_epoch,
    })
}


// ── Shared quota state ────────────────────────────────────────────────────────

struct QuotaState {
    info: Option<QuotaInfo>,
    last_ping: Instant,
    last_session_mtime: SystemTime,
}

// Re-ping when JSONL is written and this delay has passed (let API register it)
const POST_REQUEST_PING_DELAY: Duration = Duration::from_secs(4);
// Re-ping when idle for this long
const IDLE_PING_INTERVAL: Duration = Duration::from_secs(300);

// ── ClaudeProvider ────────────────────────────────────────────────────────────

pub struct ClaudeProvider {
    quota: Arc<Mutex<QuotaState>>,
}

fn do_ping_and_store(quota: &Arc<Mutex<QuotaState>>) {
    let token = match read_access_token() {
        Some((t, _)) => t,
        None => return,
    };

    let result = ping_quota(&token);

    let projects_dir = dirs_next::home_dir()
        .map(|h| h.join(".claude").join("projects"));
    let latest_mtime = projects_dir
        .as_ref()
        .and_then(|d| newest_jsonl_mtime(d))
        .unwrap_or(UNIX_EPOCH);

    let mut state = quota.lock().unwrap();
    if result.is_some() {
        state.info = result;
    }
    state.last_ping = Instant::now();
    state.last_session_mtime = latest_mtime;
}

impl ClaudeProvider {
    pub fn new() -> Self {
        let quota = Arc::new(Mutex::new(QuotaState {
            info: None,
            last_ping: Instant::now() - IDLE_PING_INTERVAL - Duration::from_secs(1),
            last_session_mtime: UNIX_EPOCH,
        }));

        // Background thread: ping immediately on start, then whenever JSONL changes or idle expires
        let quota_clone = quota.clone();
        std::thread::spawn(move || {
            // Ping immediately so reset times are correct from the first collect()
            do_ping_and_store(&quota_clone);

            loop {
                std::thread::sleep(Duration::from_secs(2));

                let need_ping = {
                    let state = quota_clone.lock().unwrap();
                    if state.last_ping.elapsed() >= IDLE_PING_INTERVAL {
                        true
                    } else {
                        // Check if JSONL was written after our last recorded mtime
                        let projects_dir = dirs_next::home_dir()
                            .map(|h| h.join(".claude").join("projects"));
                        let latest_mtime = projects_dir
                            .as_ref()
                            .and_then(|d| newest_jsonl_mtime(d))
                            .unwrap_or(UNIX_EPOCH);
                        latest_mtime > state.last_session_mtime
                            && latest_mtime.elapsed().unwrap_or(Duration::MAX) >= POST_REQUEST_PING_DELAY
                    }
                };

                if need_ping {
                    do_ping_and_store(&quota_clone);
                }
            }
        });

        Self { quota }
    }

    fn projects_path() -> Option<PathBuf> {
        let home = dirs_next::home_dir()?;
        let path = home.join(".claude").join("projects");
        if path.is_dir() { Some(path) } else { None }
    }
}

/// Returns the most-recent mtime across all JSONL files under projects_dir
fn newest_jsonl_mtime(projects_dir: &PathBuf) -> Option<SystemTime> {
    let Ok(projects) = fs::read_dir(projects_dir) else { return None };
    let mut latest = UNIX_EPOCH;
    for project in projects.flatten() {
        let pp = project.path();
        if !pp.is_dir() { continue; }
        for fp in collect_jsonl_files(&pp) {
            if let Ok(meta) = fp.metadata() {
                if let Ok(m) = meta.modified() {
                    if m > latest { latest = m; }
                }
            }
        }
    }
    if latest == UNIX_EPOCH { None } else { Some(latest) }
}

// ── AiProvider impl ───────────────────────────────────────────────────────────

impl AiProvider for ClaudeProvider {
    fn id(&self) -> &str { "claude" }
    fn display_name(&self) -> &str { "Claude Code" }
    fn process_names(&self) -> Vec<&str> { vec!["claude.exe", "claude"] }

    fn collect(&self) -> ProviderStatus {
        let running = self.is_running();

        let mut status = ProviderStatus {
            id: self.id().to_string(),
            display_name: self.display_name().to_string(),
            model: Some("claude-sonnet-4-5".to_string()),
            is_running: running,
            activity: if running { ActivityStatus::Thinking } else { ActivityStatus::Idle },
            context_window_max: Some(200_000),
            last_updated: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        };

        // ── JSONL tokens (fast — just filesystem reads) ───────────────────────
        if let Some(projects_path) = Self::projects_path() {
            let agg = collect_stats(&projects_path);

            status.session_tokens = agg.session_tokens();
            status.session_input_tokens = agg.session_input;
            status.session_output_tokens = agg.session_output;
            status.session_cache_read = agg.session_cache_read;
            status.session_cache_write = agg.session_cache_write;
            status.session_cost_usd = agg.session_cost();
            status.session_requests = agg.session_requests as u64;

            status.weekly_tokens = agg.weekly_tokens();
            status.weekly_cost_usd = agg.weekly_cost();
            status.weekly_requests = agg.weekly_requests;

            if let Some(m) = agg.session_model {
                status.model = Some(m);
            }
        }

        // ── Quota % from background ping thread (non-blocking read) ───────────
        {
            let state = self.quota.lock().unwrap();
            if let Some(q) = &state.info {
                status.session_quota_pct = Some(q.session_pct * 100.0);
                status.weekly_quota_pct = Some(q.weekly_pct * 100.0);
                status.quota_used = Some((q.weekly_pct * 100.0) as u64);
                status.quota_limit = Some(100);
                status.session_reset_epoch = q.session_reset_epoch;
                status.weekly_reset_epoch = q.weekly_reset_epoch;
            }
        }

        status
    }
}
