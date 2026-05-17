/// Resolve the real terminal HWND that hosts an AI CLI process.
///
/// Windows Terminal (wt.exe) uses a split-process model: the AI CLI lives
/// inside a conhost/openconsole child process, but the actual UI window
/// belongs to WindowsTerminal.exe, which is usually a sibling or grandparent
/// rather than a direct ancestor. A naive `EnumWindows` filter by the AI's
/// parent PID misses it. This module walks the full process tree (ancestors
/// + siblings of terminal-class processes) and ranks the candidate windows
/// before returning the best HWND.

#[cfg(target_os = "windows")]
pub use windows_impl::*;

#[cfg(not(target_os = "windows"))]
pub use stub::*;

#[cfg(target_os = "windows")]
mod windows_impl {
    use crate::process_watcher::ProcSnapshotEntry;
    use crate::window_tracker::{get_window_bounds, TerminalBounds};
    use std::collections::HashSet;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongW, GetWindowRect, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };

    /// Terminal-class process stems we treat as "hosts a terminal HWND".
    static TERMINAL_STEMS: &[&str] = &[
        "windowsterminal", "wt",
        "powershell", "pwsh", "cmd",
        "warp", "wezterm-gui", "alacritty",
        "hyper", "code",
        "conhost", "openconsole",
    ];

    fn is_terminal_stem(stem: &str) -> bool {
        TERMINAL_STEMS.iter().any(|&t| stem == t)
    }

    /// Build the candidate PID set: every terminal-class process reachable
    /// from `start_pid` through ancestor + sibling links.
    fn candidate_pids(start_pid: u32, snapshot: &[ProcSnapshotEntry]) -> HashSet<u32> {
        use std::collections::HashMap;

        let pid_to_entry: HashMap<u32, &ProcSnapshotEntry> =
            snapshot.iter().map(|e| (e.pid, e)).collect();

        // Build parent → children map for sibling lookup
        let mut children_of: HashMap<u32, Vec<u32>> = HashMap::new();
        for e in snapshot {
            children_of.entry(e.parent_pid).or_default().push(e.pid);
        }

        let mut candidates: HashSet<u32> = HashSet::new();
        let mut visited: HashSet<u32> = HashSet::new();
        let mut chain: Vec<u32> = Vec::new();

        // Walk up the ancestor chain
        let mut current = start_pid;
        for _ in 0..16 {
            if !visited.insert(current) {
                break;
            }
            chain.push(current);
            if let Some(entry) = pid_to_entry.get(&current) {
                if is_terminal_stem(&entry.stem) {
                    candidates.insert(current);
                }
                if entry.parent_pid == 0 || entry.parent_pid == current {
                    break;
                }
                current = entry.parent_pid;
            } else {
                break;
            }
        }

        // For each PID in the chain, add terminal-class siblings (same parent)
        for &pid in &chain {
            let parent = pid_to_entry.get(&pid).map(|e| e.parent_pid).unwrap_or(0);
            if parent == 0 {
                continue;
            }
            if let Some(siblings) = children_of.get(&parent) {
                for &sib in siblings {
                    if let Some(e) = pid_to_entry.get(&sib) {
                        if is_terminal_stem(&e.stem) {
                            candidates.insert(sib);
                        }
                    }
                }
            }
        }

        candidates
    }

    struct EnumCtx {
        candidate_pids: HashSet<u32>,
        results: Vec<HwndCandidate>,
    }

    #[derive(Debug, Clone)]
    struct HwndCandidate {
        hwnd_value: isize,
        #[allow(dead_code)]
        owning_pid: u32,
        has_title: bool,
        area: i64,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut EnumCtx);

        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }

        // Skip tool windows (hidden helpers like splash, IME)
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
            return BOOL(1);
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || !ctx.candidate_pids.contains(&pid) {
            return BOOL(1);
        }

        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return BOOL(1);
        }
        let w = (rect.right - rect.left) as i64;
        let h = (rect.bottom - rect.top) as i64;
        if w < 100 || h < 100 {
            return BOOL(1);
        }

        let mut title_buf = [0u16; 64];
        let title_len = GetWindowTextW(hwnd, &mut title_buf);

        ctx.results.push(HwndCandidate {
            hwnd_value: hwnd.0 as isize,
            owning_pid: pid,
            has_title: title_len > 0,
            area: w * h,
        });

        BOOL(1)
    }

    /// Resolve the best terminal HWND that hosts the AI CLI process at `ai_pid`.
    /// `terminal_pid` is the ancestor PID reported by process_watcher (may be 0).
    pub fn resolve_terminal_hwnd(
        ai_pid: u32,
        terminal_pid: u32,
        snapshot: &[ProcSnapshotEntry],
    ) -> Option<isize> {
        let mut candidates = candidate_pids(ai_pid, snapshot);
        if terminal_pid != 0 {
            for pid in candidate_pids(terminal_pid, snapshot) {
                candidates.insert(pid);
            }
            candidates.insert(terminal_pid);
        }
        if candidates.is_empty() {
            return None;
        }

        let mut ctx = EnumCtx {
            candidate_pids: candidates,
            results: Vec::new(),
        };

        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));
        }

        if ctx.results.is_empty() {
            return None;
        }

        // Rank: titled > untitled, then larger area, then lower HWND value
        ctx.results.sort_by(|a, b| {
            b.has_title.cmp(&a.has_title)
                .then(b.area.cmp(&a.area))
                .then(a.hwnd_value.cmp(&b.hwnd_value))
        });

        Some(ctx.results[0].hwnd_value)
    }

    /// Read current bounds for a previously resolved HWND.
    /// Returns `is_found = false` when the HWND is no longer a valid visible window.
    pub fn bounds_for_hwnd(hwnd_value: isize) -> TerminalBounds {
        use windows::Win32::UI::WindowsAndMessaging::IsWindow;
        let hwnd = HWND(hwnd_value as *mut _);
        if !unsafe { IsWindow(hwnd).as_bool() } {
            return TerminalBounds::default(); // is_found = false
        }
        let mut b = get_window_bounds(hwnd);
        b.is_found = true;
        b
    }
}

#[cfg(not(target_os = "windows"))]
mod stub {
    use crate::process_watcher::ProcSnapshotEntry;
    use crate::window_tracker::TerminalBounds;

    pub fn resolve_terminal_hwnd(
        _ai_pid: u32,
        _terminal_pid: u32,
        _snapshot: &[ProcSnapshotEntry],
    ) -> Option<isize> {
        None
    }

    pub fn bounds_for_hwnd(_hwnd_value: isize) -> TerminalBounds {
        TerminalBounds::default()
    }
}
