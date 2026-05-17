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
use crate::wt_tab_detector;
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

/// Hard hide via Win32. Tauri's `win.hide()` goes through the webview
/// abstraction and may be ignored or delayed for owned windows. ShowWindow
/// + SW_HIDE is synchronous and unconditional.
#[cfg(target_os = "windows")]
fn hard_hide(overlay_hwnd_value: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    if overlay_hwnd_value == 0 {
        return;
    }
    let h = HWND(overlay_hwnd_value as *mut _);
    unsafe {
        let _ = ShowWindow(h, SW_HIDE);
    }
}

/// STRICT foreground validation: returns true ONLY when the foreground
/// HWND is the exact terminal HWND that owns this overlay (or one of its
/// owned popups — e.g. a context menu of that specific terminal).
///
/// We deliberately do NOT match by:
///   - same PID (Windows Terminal hosts every tab in one wt.exe process —
///     matching by PID would show the overlay on a plain-shell tab when
///     the AI is in a different tab)
///   - ancestor process tree (would falsely match VSCode/conhost siblings)
///   - window class or title (unreliable, easily spoofed)
///
/// HWND equality is the only safe identity.
///
/// Returns (matches, foreground_hwnd_value) — second value is for diagnostic
/// logging so the caller can emit `[focus]` only on transitions.
#[cfg(target_os = "windows")]
fn check_terminal_is_foreground(terminal_hwnd_value: isize) -> (bool, isize) {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindow, GW_OWNER};
    if terminal_hwnd_value == 0 {
        return (false, 0);
    }
    unsafe {
        let fg = GetForegroundWindow();
        let fg_value = fg.0 as isize;
        if fg.0.is_null() {
            return (false, 0);
        }

        // 1. Direct HWND match.
        if fg_value == terminal_hwnd_value {
            return (true, fg_value);
        }

        // 2. Foreground HWND is an OWNED popup of the terminal.
        let mut current = fg;
        for _ in 0..8 {
            let owner = match GetWindow(current, GW_OWNER) {
                Ok(h) if !h.0.is_null() => h,
                _ => break,
            };
            if owner.0 as isize == terminal_hwnd_value {
                return (true, fg_value);
            }
            current = owner;
        }

        (false, fg_value)
    }
}

#[cfg(target_os = "windows")]
fn terminal_is_foreground(terminal_hwnd_value: isize) -> bool {
    check_terminal_is_foreground(terminal_hwnd_value).0
}

/// Check if the terminal's window rect is occluded by another window.
/// Probes interior sample points via WindowFromPoint and checks whether
/// any of them resolve to THE EXACT terminal HWND. Same-PID matches are
/// rejected — a different tab in the same wt.exe is, for our purposes,
/// a different window and counts as occlusion.
#[cfg(target_os = "windows")]
fn terminal_is_occluded(terminal_hwnd_value: isize, b: &TerminalBounds) -> bool {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, WindowFromPoint, GA_ROOT};
    if terminal_hwnd_value == 0 || b.width <= 0 || b.height <= 0 {
        return false;
    }
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
            let top = GetAncestor(hit, GA_ROOT);
            let top_value = if top.0.is_null() { hit.0 as isize } else { top.0 as isize };
            if top_value == terminal_hwnd_value {
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
fn hard_hide(_overlay_hwnd_value: isize) {}
#[cfg(not(target_os = "windows"))]
fn terminal_is_foreground(_terminal_hwnd_value: isize) -> bool { true }
#[cfg(not(target_os = "windows"))]
fn check_terminal_is_foreground(_terminal_hwnd_value: isize) -> (bool, isize) { (true, 0) }
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
    /// Whether the overlay was visible on the last tick. Used purely for
    /// transition logging — every tick still re-decides from scratch.
    pub last_visible: bool,
    /// The Windows Terminal tab name that was selected when this overlay
    /// was created. We assume the AI CLI was launched into the currently
    /// active tab. On every tick we compare against the live selected-tab
    /// name; mismatch ⇒ user switched tabs ⇒ hide overlay.
    /// `None` for non-wt terminals (where every window is its own HWND).
    pub expected_tab_name: Option<String>,
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

    // Capture the active Windows Terminal tab name at spawn time. The AI CLI
    // was just launched, so the currently-selected tab IS the one running it.
    // If this terminal isn't Windows Terminal, the call returns None and we
    // skip tab-aware visibility checks (HWND identity is sufficient).
    let expected_tab_name = wt_tab_detector::current_selected_tab_name(hwnd_value);
    if let Some(ref name) = expected_tab_name {
        println!("[overlay] captured tab name: '{}' for HWND=0x{:x}", name, hwnd_value);
    }

    reg.by_hwnd.insert(hwnd_value, OverlayInstance {
        window_label: label,
        hwnd_value,
        attached_sessions: attached,
        last_bounds: bounds,
        last_visible: false,
        expected_tab_name,
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

        // Snapshot active overlays (carry expected_tab_name too)
        let live: Vec<(isize, String, bool, Option<String>)> = {
            let reg = overlays.lock().unwrap();
            reg.by_hwnd
                .iter()
                .map(|(h, inst)| (
                    *h,
                    inst.window_label.clone(),
                    inst.last_visible,
                    inst.expected_tab_name.clone(),
                ))
                .collect()
        };

        let mut dead_hwnds: Vec<isize> = Vec::new();

        for (hwnd_value, label, last_visible, expected_tab) in live {
            // Step 1: terminal HWND must still be a valid, visible window.
            if !terminal_is_visible(hwnd_value) {
                dead_hwnds.push(hwnd_value);
                continue;
            }

            // Step 2: gather current state.
            let bounds = terminal_resolver::bounds_for_hwnd(hwnd_value);
            if !bounds.is_found || bounds.width == 0 {
                dead_hwnds.push(hwnd_value);
                continue;
            }

            // Step 3: compute strict visibility — ALL must be true:
            //   - not minimized
            //   - terminal IS the foreground window (strict HWND match)
            //   - not fully occluded
            //   - if Windows Terminal: currently selected tab name matches
            //     the tab the AI CLI was launched into
            let is_minimized = bounds.is_minimized;
            let (is_fg, fg_hwnd) = if is_minimized {
                (false, 0)
            } else {
                check_terminal_is_foreground(hwnd_value)
            };
            let occluded = is_fg && terminal_is_occluded(hwnd_value, &bounds);

            // Tab check (only meaningful when we captured one at spawn).
            // We only call UIA when terminal is foreground — UIA is heavy,
            // and tab identity doesn't matter when terminal isn't visible.
            let tab_ok = match &expected_tab {
                Some(expected) if is_fg && !occluded => {
                    match wt_tab_detector::current_selected_tab_name(hwnd_value) {
                        Some(current) => &current == expected,
                        // UIA failed — fail SAFE (hide) rather than show on wrong tab.
                        None => false,
                    }
                }
                // No tab signature captured (non-wt terminal) → trust HWND only.
                _ => true,
            };

            let should_show = !is_minimized && is_fg && !occluded && tab_ok;

            // Step 4: log transitions only.
            if should_show != last_visible {
                println!(
                    "[focus] foreground=0x{:x} terminal=0x{:x} match={}",
                    fg_hwnd as usize,
                    hwnd_value as usize,
                    is_fg
                );
                if should_show {
                    println!("[overlay] visible=true hwnd=0x{:x}", hwnd_value);
                } else {
                    let reason = if is_minimized {
                        "minimized"
                    } else if !is_fg {
                        "foreground_changed"
                    } else if occluded {
                        "occluded"
                    } else if !tab_ok {
                        "tab_changed"
                    } else {
                        "unknown"
                    };
                    println!(
                        "[overlay] visible=false hwnd=0x{:x} reason={}",
                        hwnd_value, reason
                    );
                }
            }

            // Step 5: dispatch to main thread. RE-CHECK every condition there
            // — there's a race window where foreground/tab could change.
            let app_clone = app.clone();
            let label_clone = label.clone();
            let bounds_clone = bounds.clone();
            let hwnd_for_closure = hwnd_value;
            let expected_tab_clone = expected_tab.clone();
            // Decision computed on watcher thread (already includes UIA result).
            // Main thread only needs to act — no further heavy checks there.
            let decided_show = should_show;
            let _ = app.run_on_main_thread(move || {
                let Some(win) = app_clone.get_webview_window(&label_clone) else { return };
                let overlay_hwnd_value: isize = match win.hwnd() {
                    Ok(h) => h.0 as isize,
                    Err(_) => return,
                };
                // Tiny re-check on main thread: only cheap Win32 calls
                // (no UIA — UIA on UI thread can deadlock the webview).
                let still_fg = terminal_is_foreground(hwnd_for_closure)
                    && terminal_is_visible(hwnd_for_closure);
                let act_show = decided_show && still_fg;

                let _ = (bounds_clone.clone(), expected_tab_clone.clone(), hwnd_for_closure);

                if !act_show {
                    hard_hide(overlay_hwnd_value);
                    return;
                }

                let scale = bounds_clone.scale_factor.max(0.1);
                let overlay_physical_w = (OVERLAY_LOGICAL_W * scale).round() as i32;
                let (px, py) = compute_topright(&bounds_clone, overlay_physical_w);
                let _ = win.set_position(tauri::PhysicalPosition::new(px, py));
                raise_above_owner(overlay_hwnd_value);
            });

            if let Some(inst) = overlays.lock().unwrap().by_hwnd.get_mut(&hwnd_value) {
                inst.last_bounds = bounds;
                inst.last_visible = should_show;
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
