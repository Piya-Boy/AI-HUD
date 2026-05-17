mod models;
mod providers;
mod window_tracker;
mod process_watcher;
mod terminal_resolver;
mod overlay_manager;

use models::{HudSettings, HudState};
use process_watcher::{ActiveSessions, AiSessionEvent, SharedSessions, SharedSnapshot};
use providers::ProviderManager;
use window_tracker::TerminalBounds;
use overlay_manager::SharedOverlays;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

fn build_hud_state(manager: &ProviderManager) -> HudState {
    let providers = manager.collect_all();
    let any_active = providers.iter().any(|p| p.is_running);
    let active_count = providers.iter().filter(|p| p.is_running).count() as u32;

    let total_session_tokens: u64 = providers.iter().map(|p| p.session_tokens).sum();
    let total_daily_tokens: u64 = providers.iter().map(|p| p.daily_tokens).sum();
    let total_weekly_tokens: u64 = providers.iter().map(|p| p.weekly_tokens).sum();
    let total_session_cost: f64 = providers.iter().map(|p| p.session_cost_usd).sum();
    let total_daily_cost: f64 = providers.iter().map(|p| p.daily_cost_usd).sum();
    let total_weekly_cost: f64 = providers.iter().map(|p| p.weekly_cost_usd).sum();

    HudState {
        providers,
        any_active,
        total_session_tokens,
        total_daily_tokens,
        total_weekly_tokens,
        total_session_cost,
        total_daily_cost,
        total_weekly_cost,
        active_count,
        last_updated: chrono::Utc::now().to_rfc3339(),
    }
}

#[tauri::command]
async fn get_hud_state(
    manager: tauri::State<'_, Arc<Mutex<ProviderManager>>>,
) -> Result<HudState, String> {
    let mgr = manager.lock().map_err(|e| e.to_string())?;
    Ok(build_hud_state(&mgr))
}

#[tauri::command]
async fn get_settings(
    settings: tauri::State<'_, Arc<Mutex<HudSettings>>>,
) -> Result<HudSettings, String> {
    let s = settings.lock().map_err(|e| e.to_string())?;
    Ok(s.clone())
}

#[tauri::command]
async fn update_settings(
    new_settings: HudSettings,
    settings: tauri::State<'_, Arc<Mutex<HudSettings>>>,
) -> Result<(), String> {
    let mut s = settings.lock().map_err(|e| e.to_string())?;
    *s = new_settings;
    if let Some(config_dir) = dirs_next::config_dir() {
        let dir = config_dir.join("ai-hud");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("settings.json");
        if let Ok(json) = serde_json::to_string_pretty(&*s) {
            let _ = std::fs::write(path, json);
        }
    }
    Ok(())
}

#[tauri::command]
async fn get_bounds_for_pid(pid: u32) -> Result<Option<TerminalBounds>, String> {
    Ok(window_tracker::get_bounds_for_terminal_pid(pid))
}

#[tauri::command]
async fn get_active_sessions(
    sessions: tauri::State<'_, SharedSessions>,
) -> Result<ActiveSessions, String> {
    Ok(process_watcher::get_active_sessions(&sessions))
}

#[tauri::command]
async fn start_drag(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_click_through(window: tauri::Window, enabled: bool) -> Result<(), String> {
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| e.to_string())
}

fn load_settings() -> HudSettings {
    if let Some(config_dir) = dirs_next::config_dir() {
        let path = config_dir.join("ai-hud").join("settings.json");
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(s) = serde_json::from_str::<HudSettings>(&content) {
                return s;
            }
        }
    }
    HudSettings::default()
}

fn start_polling(
    app: AppHandle,
    manager: Arc<Mutex<ProviderManager>>,
    settings: Arc<Mutex<HudSettings>>,
    running: Arc<AtomicBool>,
) {
    std::thread::spawn(move || loop {
        let interval = {
            let s = settings.lock().unwrap();
            s.poll_interval_ms
        };
        std::thread::sleep(Duration::from_millis(interval));

        if !running.load(Ordering::Relaxed) {
            break;
        }

        let state = {
            let mgr = manager.lock().unwrap();
            build_hud_state(&mgr)
        };

        let _ = app.emit("hud-update", &state);
    });
}

/// Wire the process watcher to the OverlayManager:
/// every detected session spawns its own overlay window;
/// every exit closes that window.
fn start_process_watcher(
    app: AppHandle,
    sessions: SharedSessions,
    snapshot: SharedSnapshot,
    overlays: SharedOverlays,
) {
    let app_start = app.clone();
    let overlays_start = overlays.clone();
    let app_stop = app.clone();
    let overlays_stop = overlays.clone();

    process_watcher::start_watcher(
        sessions,
        snapshot,
        move |event: AiSessionEvent, snap| {
            let _ = app_start.emit("ai-session-started", &event);
            overlay_manager::spawn_overlay(&app_start, &overlays_start, &event, snap);
        },
        move |event: AiSessionEvent| {
            let _ = app_stop.emit("ai-session-stopped", &event);
            overlay_manager::close_overlay(&app_stop, &overlays_stop, &event.session_id);
        },
    );
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    let settings = Arc::new(Mutex::new(load_settings()));
    let settings_clone = settings.clone();

    let mut mgr = ProviderManager::new();
    mgr.register_defaults();
    let manager = Arc::new(Mutex::new(mgr));
    let manager_clone = manager.clone();

    let shared_sessions = process_watcher::new_shared_sessions();
    let shared_snapshot = process_watcher::new_shared_snapshot();
    let shared_overlays = overlay_manager::new_shared_overlays();

    let sessions_clone = shared_sessions.clone();
    let snapshot_clone = shared_snapshot.clone();
    let snapshot_anchor_clone = shared_snapshot.clone();
    let overlays_clone = shared_overlays.clone();
    let overlays_anchor_clone = shared_overlays.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(manager.clone())
        .manage(settings.clone())
        .manage(shared_sessions.clone())
        .manage(shared_overlays.clone())
        .invoke_handler(tauri::generate_handler![
            get_hud_state,
            get_settings,
            update_settings,
            get_bounds_for_pid,
            get_active_sessions,
            start_drag,
            set_click_through,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let handle2 = handle.clone();
            let handle3 = handle.clone();
            start_polling(handle, manager_clone, settings_clone, running_clone);
            start_process_watcher(handle2, sessions_clone, snapshot_clone, overlays_clone);
            overlay_manager::start_anchor_loop(handle3, overlays_anchor_clone, snapshot_anchor_clone);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    running.store(false, Ordering::Relaxed);
}
