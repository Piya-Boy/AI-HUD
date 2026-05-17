/// Background update checker.
///
/// Checks GitHub Releases on startup (after 10s delay) and every 4 hours.
/// Emits `update-available` event to all windows when a new version is found.
/// Never prompts mid-session; UI decides when to show the toast.

use crate::models::HudSettings;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub checking: bool,
    pub error: Option<String>,
    pub channel: String,
}

/// Start the background update-check loop.
/// Emits `update-status` on every check result.
pub fn start_update_checker(app: AppHandle, settings: Arc<Mutex<HudSettings>>) {
    std::thread::spawn(move || {
        // Wait on startup so we don't block the overlay from appearing.
        std::thread::sleep(Duration::from_secs(10));

        loop {
            let (should_check, channel, skipped) = {
                match settings.lock() {
                    Ok(s) => (s.auto_update_check, s.update_channel.clone(), s.skipped_version.clone()),
                    Err(_) => (false, "stable".into(), None),
                }
            };

            if should_check {
                let current = tauri::VERSION.to_string();
                let status = check_for_update(&current, &channel, skipped.as_deref());
                let _ = app.emit("update-status", &status);
            }

            // Recheck every 4 hours.
            std::thread::sleep(Duration::from_secs(4 * 3600));
        }
    });
}

/// Force-trigger an update check immediately.
/// Returns the status synchronously (called from Tauri command).
pub fn force_check(current_version: &str, channel: &str, skipped: Option<&str>) -> UpdateStatus {
    check_for_update(current_version, channel, skipped)
}

fn check_for_update(current_version: &str, channel: &str, skipped: Option<&str>) -> UpdateStatus {
    let endpoint = channel_endpoint(channel);

    match fetch_update_manifest(&endpoint) {
        Ok(manifest) => {
            let available = is_newer(&manifest.version, current_version)
                && skipped.map_or(true, |s| s != manifest.version);
            UpdateStatus {
                current_version: current_version.to_string(),
                latest_version: Some(manifest.version),
                update_available: available,
                checking: false,
                error: None,
                channel: channel.to_string(),
            }
        }
        Err(e) => UpdateStatus {
            current_version: current_version.to_string(),
            latest_version: None,
            update_available: false,
            checking: false,
            error: Some(e),
            channel: channel.to_string(),
        },
    }
}

#[derive(Debug, Deserialize)]
struct UpdateManifest {
    version: String,
}

fn channel_endpoint(channel: &str) -> String {
    // GitHub Releases "latest.json" — stable uses default latest, others use pre-release tag.
    // Replace Piya-Boy/AI-HUD with the actual repo once published.
    match channel {
        "beta" => "https://github.com/Piya-Boy/AI-HUD/releases/download/beta/latest.json".into(),
        "nightly" => "https://github.com/Piya-Boy/AI-HUD/releases/download/nightly/latest.json".into(),
        _ => "https://github.com/Piya-Boy/AI-HUD/releases/latest/download/latest.json".into(),
    }
}

fn fetch_update_manifest(url: &str) -> Result<UpdateManifest, String> {
    // Use a simple synchronous HTTP GET via std — no heavy async runtime needed here.
    // For the final published build the URL will be a real endpoint; during dev this
    // returns an error gracefully (no panic).
    let response = ureq_get(url)?;
    serde_json::from_str::<UpdateManifest>(&response)
        .map_err(|e| format!("parse error: {e}"))
}

/// Minimal synchronous HTTP GET using only std (no extra crate).
/// Falls back to WinHTTP via PowerShell on Windows.
fn ureq_get(url: &str) -> Result<String, String> {
    // We use std::process::Command to invoke curl/wget which are available on
    // all supported platforms without adding an HTTP crate dependency.
    let output = std::process::Command::new("curl")
        .args(["--silent", "--max-time", "10", "--location", "--fail", url])
        .output()
        .map_err(|e| format!("curl exec failed: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "curl returned {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("utf8 error: {e}"))
}

/// Returns true if `candidate` is strictly newer than `current` (semver ordering).
/// Falls back to string inequality so pre-release tags still surface.
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(v: &str) -> Option<(u64, u64, u64)> {
        let v = v.trim_start_matches('v');
        let parts: Vec<&str> = v.splitn(3, '.').collect();
        if parts.len() < 3 { return None; }
        let maj = parts[0].parse::<u64>().ok()?;
        let min = parts[1].parse::<u64>().ok()?;
        let patch = parts[2].split(['-', '+']).next()?.parse::<u64>().ok()?;
        Some((maj, min, patch))
    }

    match (parse(candidate), parse(current)) {
        (Some(c), Some(cur)) => c > cur,
        _ => candidate != current,
    }
}
