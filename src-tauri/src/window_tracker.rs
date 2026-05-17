/// Terminal window bounds lookup by HWND or PID.

#[cfg(target_os = "windows")]
pub use windows_impl::*;

#[cfg(not(target_os = "windows"))]
pub use stub::*;

// ── Shared types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TerminalBounds {
    /// Physical pixel coordinates from GetWindowRect
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub is_minimized: bool,
    pub is_found: bool,
    pub terminal_name: String,
    /// DPI scale factor (e.g. 1.5 for 150%)
    pub scale_factor: f64,
}

impl Default for TerminalBounds {
    fn default() -> Self {
        Self {
            x: 20,
            y: 60,
            width: 0,
            height: 0,
            is_minimized: false,
            is_found: false,
            terminal_name: String::new(),
            scale_factor: 1.0,
        }
    }
}

// ── Windows implementation ───────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::TerminalBounds;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible, SW_SHOWMINIMIZED,
        GetWindowPlacement, WINDOWPLACEMENT,
    };
    use windows::Win32::UI::HiDpi::GetDpiForWindow;

    struct FindByPidState {
        target_pid: u32,
        found: Option<HWND>,
    }

    unsafe extern "system" fn enum_by_pid_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut FindByPidState);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == state.target_pid {
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if width > 100 && height > 100 {
                state.found = Some(hwnd);
                return BOOL(0);
            }
        }
        BOOL(1)
    }

    pub fn find_window_by_pid(pid: u32) -> Option<HWND> {
        if pid == 0 {
            return None;
        }
        let mut state = FindByPidState { target_pid: pid, found: None };
        unsafe {
            let _ = EnumWindows(
                Some(enum_by_pid_proc),
                LPARAM(&mut state as *mut _ as isize),
            );
        }
        state.found
    }

    /// Get bounds for a specific terminal PID.
    pub fn get_bounds_for_terminal_pid(pid: u32) -> Option<TerminalBounds> {
        let hwnd = find_window_by_pid(pid)?;
        let mut b = get_window_bounds(hwnd);
        b.is_found = true;
        Some(b)
    }

    /// Read the current bounds of a window handle.
    pub fn get_window_bounds(hwnd: HWND) -> TerminalBounds {
        unsafe {
            let mut wp = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            let _ = GetWindowPlacement(hwnd, &mut wp);
            let is_minimized = wp.showCmd == SW_SHOWMINIMIZED.0 as u32 || IsIconic(hwnd).as_bool();

            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return TerminalBounds::default();
            }

            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            let dpi = GetDpiForWindow(hwnd);
            let scale_factor = if dpi > 0 { dpi as f64 / 96.0 } else { 1.0 };

            TerminalBounds {
                x: rect.left,
                y: rect.top,
                width,
                height,
                is_minimized,
                is_found: true,
                terminal_name: String::new(),
                scale_factor,
            }
        }
    }
}

// ── Stub for non-Windows ─────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
mod stub {
    use super::TerminalBounds;

    pub fn get_bounds_for_terminal_pid(_pid: u32) -> Option<TerminalBounds> {
        None
    }
}
