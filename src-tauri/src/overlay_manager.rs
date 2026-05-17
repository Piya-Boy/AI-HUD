/// OverlayManager — one Tauri WebviewWindow per **terminal HWND**, not per
/// AI process. Multiple AI sessions sharing the same terminal share the
/// same overlay; the overlay outlives any single AI process and is only
/// closed when:
///   - the host terminal HWND becomes invalid (window destroyed), OR
///   - the last AI session attached to that HWND exits.
///
/// Identity:
///   primary key   = terminal HWND (canonical, dedupe boundary)
///   attached set  = session_ids currently using this overlay
///
/// Dedupe guards:
///   1. HWND registry: lookup before spawn, never create twice.
///   2. Per-HWND cooldown: ignore spawn requests within 500 ms of a prior
///      spawn (handles WebviewWindowBuilder running async on main thread
///      where a duplicate ai-session-started can race).
///   3. Empty-HWND quarantine: sessions whose resolver returns 0 are
///      tracked but do NOT spawn a window — they wait for a later
///      retry tick. Prevents stacking overlays at the fallback coords.

use crate::process_watcher::{AiSessionEvent, ProcSnapshotEntry, SharedSnapshot};
use crate::terminal_resolver;
use crate::window_tracker::TerminalBounds;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Apply native OS window styles that make the overlay behave like a HUD:
///   - click-through (mouse passes through to whatever is under it)
///   - never steals focus / never appears in ALT+TAB
///   - layered for compositing
///   - explicitly NOT topmost (we manage z-order via owner relationship)
#[cfg(target_os = "windows")]
fn apply_hud_styles(win: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE,
        WS_EX_LAYERED, WS_EX_TRANSPARENT, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST,
    };
    let Ok(hwnd_raw) = win.hwnd() else { return };
    let hwnd = HWND(hwnd_raw.0 as *mut _);
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        // Strip topmost (Tauri's always_on_top may have set it) and add HUD bits.
        let new_style = (current & !WS_EX_TOPMOST.0)
            | WS_EX_LAYERED.0
            | WS_EX_TRANSPARENT.0
            | WS_EX_NOACTIVATE.0
            | WS_EX_TOOLWINDOW.0;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style as isize);
    }
}

/// Make the overlay an OWNED window of the terminal. This is the key to
/// embedded-feel z-order: an owned window stays above its owner but moves
/// with the owner in z-order — when the terminal is buried, so is the overlay.
/// We use GWLP_HWNDPARENT (owner relationship), NOT SetParent (child clipping).
#[cfg(target_os = "windows")]
fn set_owner(overlay_hwnd_value: isize, terminal_hwnd_value: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_HWNDPARENT};
    if terminal_hwnd_value == 0 {
        return;
    }
    let overlay = HWND(overlay_hwnd_value as *mut _);
    unsafe {
        SetWindowLongPtrW(overlay, GWLP_HWNDPARENT, terminal_hwnd_value);
    }
}

/// Re-position the overlay just above its owner in z-order (NON-topmost),
/// without activating it. Called every anchor tick when the terminal is
/// the foreground window.
#[cfg(target_os = "windows")]
fn raise_above_owner(overlay_hwnd_value: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE,
        SWP_SHOWWINDOW,
    };
    let overlay = HWND(overlay_hwnd_value as *mut _);
    unsafe {
        let _ = SetWindowPos(
            overlay,
            HWND_NOTOPMOST,
            0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// Strict foreground validation. Returns true ONLY when the user is
/// currently interacting with the terminal that owns this overlay.
///
/// Test order:
///   1. Foreground HWND == terminal HWND  → yes (direct match)
///   2. Foreground HWND's owner chain contains terminal HWND → yes (popup of terminal)
///   3. Foreground HWND belongs to the SAME PROCESS as terminal HWND → yes
///      (covers Windows Terminal: different HWND per tab but same wt.exe PID)
///   4. Otherwise → NO. Overlay must hide.
///
/// We deliberately do NOT use ancestor process tree matching — that would
/// let VSCode show the overlay because VSCode hosts conhost which is in
/// the same process tree as the AI CLI. Strict PID equality only.
#[cfg(target_os = "windows")]
fn terminal_is_foreground(terminal_hwnd_value: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindow, GetWindowThreadProcessId, GW_OWNER,
    };
    if terminal_hwnd_value == 0 {
        return false;
    }
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return false;
        }

        // 1. Direct HWND match
        if fg.0 as isize == terminal_hwnd_value {
            return true;
        }

        // 2. Owner-chain match
        let mut current = fg;
        for _ in 0..8 {
            let owner = match GetWindow(current, GW_OWNER) {
                Ok(h) if !h.0.is_null() => h,
                _ => break,
            };
            if owner.0 as isize == terminal_hwnd_value {
                return true;
            }
            current = owner;
        }

        // 3. Same-PID match (Windows Terminal multi-HWND-per-tab case)
        let terminal_hwnd = HWND(terminal_hwnd_value as *mut _);
        let mut fg_pid: u32 = 0;
        let mut term_pid: u32 = 0;
        GetWindowThreadProcessId(fg, Some(&mut fg_pid));
        GetWindowThreadProcessId(terminal_hwnd, Some(&mut term_pid));
        if fg_pid != 0 && term_pid != 0 && fg_pid == term_pid {
            return true;
        }

        false
    }
}

/// Check if any portion of the terminal's window rect is actually visible
/// on screen (not fully occluded by another window). Uses a simple sampling
/// strategy: pick a few interior points and ask Windows what HWND is there.
/// If none of them resolve to the terminal HWND or one of its child/owned
/// windows, treat the terminal as occluded.
#[cfg(target_os = "windows")]
fn terminal_is_occluded(terminal_hwnd_value: isize, b: &TerminalBounds) -> bool {
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, WindowFromPoint, GA_ROOT};
    if terminal_hwnd_value == 0 || b.width <= 0 || b.height <= 0 {
        return false;
    }
    // Probe four spread-out interior points (avoid edges due to DWM shadow).
    let inset = 16;
    let pts = [
        (b.x + inset,            b.y + inset),
        (b.x + b.width - inset,  b.y + inset),
        (b.x + b.width / 2,      b.y + b.height / 2),
        (b.x + b.width - inset,  b.y + b.height - inset),
    ];
    unsafe {
        for (px, py) in pts {
            let pt = POINT { x: px, y: py };
            let hit = WindowFromPoint(pt);
            if hit.0.is_null() {
                continue;
            }
            // Walk to top-level so we compare to terminal_hwnd_value (a top-level HWND).
            let top = GetAncestor(hit, GA_ROOT);
            let top_value = if top.0.is_null() { hit.0 as isize } else { top.0 as isize };
            if top_value == terminal_hwnd_value {
                return false; // at least one sample point is owned by the terminal
            }
            // Same-process check (Windows Terminal sub-HWNDs)
            use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
            let mut hit_pid: u32 = 0;
            let mut term_pid: u32 = 0;
            GetWindowThreadProcessId(HWND(top_value as *mut _), Some(&mut hit_pid));
            GetWindowThreadProcessId(HWND(terminal_hwnd_value as *mut _), Some(&mut term_pid));
            if hit_pid != 0 && hit_pid == term_pid {
                return false;
            }
        }
    }
    true
}

/// Returns true if the terminal HWND still corresponds to a visible,
/// non-destroyed top-level window.
#[cfg(target_os = "windows")]
fn terminal_is_visible(terminal_hwnd_value: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{IsWindow, IsWindowVisible};
    if terminal_hwnd_value == 0 {
        return false;
    }
    let h = HWND(terminal_hwnd_value as *mut _);
    unsafe { IsWindow(h).as_bool() && IsWindowVisible(h).as_bool() }
}

#[cfg(not(target_os = "windows"))]
fn apply_hud_styles(_win: &WebviewWindow) {}
#[cfg(not(target_os = "windows"))]
fn set_owner(_overlay_hwnd_value: isize, _terminal_hwnd_value: isize) {}
#[cfg(not(target_os = "windows"))]
fn raise_above_owner(_overlay_hwnd_value: isize) {}
#[cfg(not(target_os = "windows"))]
fn terminal_is_foreground(_terminal_hwnd_value: isize) -> bool { true }
#[cfg(not(target_os = "windows"))]
fn terminal_is_occluded(_terminal_hwnd_value: isize, _b: &TerminalBounds) -> bool { false }
#[cfg(not(target_os = "windows"))]
fn terminal_is_visible(_terminal_hwnd_value: isize) -> bool { true }

const OVERLAY_LOGICAL_W: f64 = 220.0;
const OVERLAY_LOGICAL_H: f64 = 250.0;
const DWM_SHADOW_PHYSICAL: i32 = 8;
/// Inset from the right edge of the terminal.
const MARGIN_RIGHT_LOGICAL: i32 = 16;
/// Inset from the top edge — large enough to clear the terminal title bar.
const MARGIN_TOP_LOGICAL: i32 = 48;

#[derive(Debug)]
pub struct OverlayInstance {
    pub window_label: String,
    #[allow(dead_code)]
    pub hwnd_value: isize,
    pub attached_sessions: HashSet<String>,
    pub last_bounds: TerminalBounds,
    /// Cached: was the terminal the foreground window on the last tick?
    /// Drives hide/show transitions without spamming visibility changes.
    pub last_foreground: bool,
    #[allow(dead_code)]
    pub last_spawn: Instant,
    #[allow(dead_code)]
    pub provider_id_for_url: String,
}

#[derive(Default)]
pub struct OverlayRegistry {
    /// HWND → overlay instance. THIS is the dedupe boundary.
    by_hwnd: HashMap<isize, OverlayInstance>,
    /// session_id → HWND. Reverse index so close_overlay can find the
    /// owning HWND without scanning.
    session_to_hwnd: HashMap<String, isize>,
    /// Sessions whose HWND we couldn't resolve yet. Re-tried by anchor loop.
    pending_sessions: HashMap<String, AiSessionEvent>,
}

pub type SharedOverlays = Arc<Mutex<OverlayRegistry>>;

pub fn new_shared_overlays() -> SharedOverlays {
    Arc::new(Mutex::new(OverlayRegistry::default()))
}

fn make_label_for_hwnd(hwnd_value: isize) -> String {
    format!("overlay-hwnd-{:x}", hwnd_value as usize)
}

fn compute_topright(b: &TerminalBounds, overlay_physical_w: i32) -> (i32, i32) {
    let scale = b.scale_factor.max(0.1);
    let shadow = (DWM_SHADOW_PHYSICAL as f64 * scale).round() as i32;
    let margin_right = (MARGIN_RIGHT_LOGICAL as f64 * scale).round() as i32;
    let margin_top = (MARGIN_TOP_LOGICAL as f64 * scale).round() as i32;
    let visible_right = b.x + b.width - shadow;
    let visible_top = b.y + shadow;
    let x = visible_right - overlay_physical_w - margin_right;
    let y = visible_top + margin_top;
    (x, y)
}

fn build_window(
    app: &AppHandle,
    label: &str,
    provider_id: &str,
    session_id: &str,
    terminal_pid: u32,
    ai_pid: u32,
    hwnd_value: isize,
    px: i32,
    py: i32,
    scale: f64,
) {
    let url_path = format!(
        "index.html?session_id={}&provider_id={}&terminal_pid={}&ai_pid={}&hwnd={:x}",
        urlencoding(session_id),
        urlencoding(provider_id),
        terminal_pid,
        ai_pid,
        hwnd_value as usize,
    );

    let app_clone = app.clone();
    let label_owned = label.to_string();
    let provider_owned = provider_id.to_string();

    let _ = app.run_on_main_thread(move || {
        // Double-check on the main thread: another spawn may have completed
        // between the cooldown check and now.
        if app_clone.get_webview_window(&label_owned).is_some() {
            return;
        }

        let result = WebviewWindowBuilder::new(
            &app_clone,
            &label_owned,
            WebviewUrl::App(url_path.into()),
        )
        .title(format!("AI-HUD · {}", provider_owned))
        .inner_size(OVERLAY_LOGICAL_W, OVERLAY_LOGICAL_H)
        .position(px as f64 / scale, py as f64 / scale)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .build();

        match result {
            Ok(win) => {
                let _ = win.set_ignore_cursor_events(true);
                // Strip TOPMOST + add LAYERED/TRANSPARENT/NOACTIVATE/TOOLWINDOW
                apply_hud_styles(&win);
                // Attach as OWNED window of the terminal — drives z-order, minimize, occlusion.
                if let Ok(overlay_hwnd_raw) = win.hwnd() {
                    let overlay_hwnd_value = overlay_hwnd_raw.0 as isize;
                    set_owner(overlay_hwnd_value, hwnd_value);
                }
                let _ = win.show();
                println!("[overlay] created for HWND=0x{:x} (owned)", hwnd_value);
            }
            Err(e) => {
                println!("[overlay] build failed for {}: {}", label_owned, e);
            }
        }
    });
}

/// Try to spawn or attach a session to its overlay. Idempotent: safe to
/// call multiple times for the same session_id without producing duplicates.
pub fn spawn_overlay(
    app: &AppHandle,
    overlays: &SharedOverlays,
    event: &AiSessionEvent,
    snapshot: &[ProcSnapshotEntry],
) {
    // Step 1: resolve HWND for this session
    let hwnd_value = terminal_resolver::resolve_terminal_hwnd(
        event.pid,
        event.terminal_pid,
        snapshot,
    )
    .unwrap_or(0);

    let mut reg = overlays.lock().unwrap();

    if reg.session_to_hwnd.contains_key(&event.session_id) {
        return;
    }

    if hwnd_value == 0 {
        reg.pending_sessions.insert(event.session_id.clone(), event.clone());
        return;
    }

    if reg.by_hwnd.contains_key(&hwnd_value) {
        let existing = reg.by_hwnd.get_mut(&hwnd_value).unwrap();
        existing.attached_sessions.insert(event.session_id.clone());
        reg.session_to_hwnd.insert(event.session_id.clone(), hwnd_value);
        println!("[overlay] attached session to existing HWND=0x{:x}", hwnd_value);
        return;
    }

    // Step 5: brand-new HWND → spawn one window
    let label = make_label_for_hwnd(hwnd_value);
    let bounds = terminal_resolver::bounds_for_hwnd(hwnd_value);
    let scale = bounds.scale_factor.max(0.1);
    let overlay_physical_w = (OVERLAY_LOGICAL_W * scale).round() as i32;

    let (px, py) = if bounds.is_found && bounds.width > 0 {
        compute_topright(&bounds, overlay_physical_w)
    } else {
        (1600, 60)
    };

    build_window(
        app, &label, &event.provider_id, &event.session_id,
        event.terminal_pid, event.pid, hwnd_value, px, py, scale,
    );

    let mut attached = HashSet::new();
    attached.insert(event.session_id.clone());

    reg.by_hwnd.insert(hwnd_value, OverlayInstance {
        window_label: label,
        hwnd_value,
        attached_sessions: attached,
        last_bounds: bounds,
        last_foreground: false,
        last_spawn: Instant::now(),
        provider_id_for_url: event.provider_id.clone(),
    });
    reg.session_to_hwnd.insert(event.session_id.clone(), hwnd_value);
    reg.pending_sessions.remove(&event.session_id);
}

/// Called when an AI CLI process exits. Removes the session from its
/// overlay; closes the overlay only when no sessions remain attached.
pub fn close_overlay(app: &AppHandle, overlays: &SharedOverlays, session_id: &str) {
    let mut reg = overlays.lock().unwrap();
    reg.pending_sessions.remove(session_id);

    let Some(hwnd_value) = reg.session_to_hwnd.remove(session_id) else {
        return;
    };

    let mut close_label: Option<String> = None;
    if let Some(instance) = reg.by_hwnd.get_mut(&hwnd_value) {
        instance.attached_sessions.remove(session_id);
        if instance.attached_sessions.is_empty() {
            close_label = Some(instance.window_label.clone());
        }
    }

    if let Some(label) = close_label {
        reg.by_hwnd.remove(&hwnd_value);
        let app_clone = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(win) = app_clone.get_webview_window(&label) {
                let _ = win.close();
                println!("[overlay] destroyed HWND=0x{:x}", hwnd_value);
            }
        });
    }
}

/// Spawned at startup: re-anchors live overlays at ~30 Hz and retries
/// pending (unresolved) sessions every tick. Also reaps overlays whose
/// HWND has been destroyed by the OS.
pub fn start_anchor_loop(app: AppHandle, overlays: SharedOverlays, snapshot: SharedSnapshot) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(33));

        // Retry pending sessions that didn't have a resolvable HWND at spawn time.
        let pending_snapshot: Vec<AiSessionEvent> = {
            let reg = overlays.lock().unwrap();
            reg.pending_sessions.values().cloned().collect()
        };

        if !pending_snapshot.is_empty() {
            let snap = snapshot.lock().unwrap().clone();
            for event in pending_snapshot {
                // spawn_overlay is idempotent — it re-checks session_to_hwnd first.
                spawn_overlay(&app, &overlays, &event, &snap);
            }
        }

        // Snapshot active overlays (bounds + last foreground state)
        let live: Vec<(isize, String, TerminalBounds, bool)> = {
            let reg = overlays.lock().unwrap();
            reg.by_hwnd
                .iter()
                .map(|(h, inst)| (
                    *h,
                    inst.window_label.clone(),
                    inst.last_bounds.clone(),
                    inst.last_foreground,
                ))
                .collect()
        };

        let mut dead_hwnds: Vec<isize> = Vec::new();

        for (hwnd_value, label, last_bounds, last_fg) in live {
            let bounds = terminal_resolver::bounds_for_hwnd(hwnd_value);
            if !bounds.is_found || bounds.width == 0 {
                dead_hwnds.push(hwnd_value);
                continue;
            }

            let is_fg = terminal_is_foreground(hwnd_value);
            let bounds_changed = bounds != last_bounds;
            let fg_changed = is_fg != last_fg;

            // Log significant transitions
            if bounds.is_minimized && !last_bounds.is_minimized {
                println!("[overlay] minimized HWND=0x{:x}", hwnd_value);
            } else if !bounds.is_minimized && last_bounds.is_minimized {
                println!("[overlay] restored HWND=0x{:x}", hwnd_value);
            }
            if fg_changed {
                println!(
                    "[overlay] terminal HWND=0x{:x} foreground={}",
                    hwnd_value, is_fg
                );
            }

            // Always re-evaluate visibility every tick when foreground.
            // Skip only when fully stable AND backgrounded (already hidden).
            if !bounds_changed && !fg_changed && !is_fg {
                continue;
            }

            let app_clone = app.clone();
            let label_clone = label.clone();
            let bounds_clone = bounds.clone();
            let _ = app.run_on_main_thread(move || {
                let Some(win) = app_clone.get_webview_window(&label_clone) else { return };

                // HIDE when terminal is minimized OR not the foreground window.
                // The foreground gate prevents the overlay from floating above
                // unrelated apps (VSCode, browsers, fullscreen apps).
                if bounds_clone.is_minimized || !is_fg {
                    let _ = win.hide();
                    return;
                }

                // Reposition into top-right of terminal visible area.
                let scale = bounds_clone.scale_factor.max(0.1);
                let overlay_physical_w = (OVERLAY_LOGICAL_W * scale).round() as i32;
                let (px, py) = compute_topright(&bounds_clone, overlay_physical_w);
                let _ = win.set_position(tauri::PhysicalPosition::new(px, py));

                // Raise above owner (NON-topmost) — anchors overlay into
                // the terminal's z-order layer, not the global topmost layer.
                if let Ok(overlay_hwnd_raw) = win.hwnd() {
                    raise_above_owner(overlay_hwnd_raw.0 as isize);
                } else {
                    let _ = win.show();
                }
            });

            if let Some(inst) = overlays.lock().unwrap().by_hwnd.get_mut(&hwnd_value) {
                inst.last_bounds = bounds;
                inst.last_foreground = is_fg;
            }
        }

        // Reap dead HWNDs (window closed by user)
        if !dead_hwnds.is_empty() {
            let mut reg = overlays.lock().unwrap();
            for hwnd_value in dead_hwnds {
                if let Some(inst) = reg.by_hwnd.remove(&hwnd_value) {
                    for sid in &inst.attached_sessions {
                        reg.session_to_hwnd.remove(sid);
                    }
                    let label = inst.window_label.clone();
                    let app_clone = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        if let Some(win) = app_clone.get_webview_window(&label) {
                            let _ = win.close();
                        }
                    });
                    println!("[overlay] terminal closed → destroyed HWND=0x{:x}", hwnd_value);
                }
            }
        }

    });
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            _ => out.push_str(&format!("%{:02X}", c as u32)),
        }
    }
    out
}
