/// Process watcher — detects AI CLI tool start/stop events
///
/// Polls running processes every 200 ms. When a new AI CLI PID appears
/// it emits "ai-session-started"; when it disappears it emits "ai-session-stopped".
/// Each event carries an AiSessionEvent payload that includes which provider
/// matched and the PID of the parent terminal window (Windows only).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ── Public types ──────────────────────────────────────────────────────────────

/// Emitted to the frontend when an AI CLI session starts or stops.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSessionEvent {
    /// Unique session ID (provider_id + "_" + pid)
    pub session_id: String,
    /// Provider identifier (e.g. "claude", "gemini")
    pub provider_id: String,
    /// OS process ID of the AI CLI process
    pub pid: u32,
    /// PID of the terminal that owns this process (0 = unknown)
    pub terminal_pid: u32,
    /// Executable name (e.g. "claude.exe")
    pub exe_name: String,
}

/// How long an AI process must be continuously alive before we treat it as
/// a real session (avoids flicker from short-lived build-tool child processes).
const STABILITY_MS: u64 = 500;

/// Snapshot of all currently tracked sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSessions {
    pub sessions: Vec<AiSessionEvent>,
}

/// Public, OS-agnostic process snapshot entry consumed by `terminal_resolver`.
#[derive(Debug, Clone)]
pub struct ProcSnapshotEntry {
    pub pid: u32,
    pub parent_pid: u32,
    pub stem: String,
}

/// Internal per-process record
#[derive(Debug, Clone)]
pub(crate) struct TrackedProcess {
    pub(crate) session_id: String,
    pub(crate) provider_id: String,
    pub(crate) terminal_pid: u32,
    pub(crate) exe_name: String,
    /// When we first observed this process — used for stability delay.
    pub(crate) first_seen: std::time::Instant,
    /// Whether on_started has already been fired for this process.
    pub(crate) announced: bool,
}

// ── Known AI CLI process names (lowercase, without .exe) ─────────────────────

struct ProviderEntry {
    id: &'static str,
    names: &'static [&'static str],
}

static PROVIDERS: &[ProviderEntry] = &[
    ProviderEntry { id: "claude",     names: &["claude"] },
    ProviderEntry { id: "gemini",     names: &["gemini", "gemini-cli"] },
    ProviderEntry { id: "openai",     names: &["codex", "openai"] },
    ProviderEntry { id: "aider",      names: &["aider"] },
    ProviderEntry { id: "ollama",     names: &["ollama"] },
    ProviderEntry { id: "continue",   names: &["continue"] },
    ProviderEntry { id: "openrouter", names: &["openrouter", "or"] },
];

fn match_provider(stem: &str) -> Option<&'static str> {
    let lower = stem.to_lowercase();
    for entry in PROVIDERS {
        if entry.names.iter().any(|&n| lower == n) {
            return Some(entry.id);
        }
    }
    None
}

// ── Shared state exposed to lib.rs ───────────────────────────────────────────

pub type SharedSessions = Arc<Mutex<HashMap<u32, TrackedProcess>>>;

pub fn new_shared_sessions() -> SharedSessions {
    Arc::new(Mutex::new(HashMap::new()))
}

// ── Platform-specific scanning ────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    /// One entry from a process snapshot
    pub struct ProcEntry {
        pub pid: u32,
        pub parent_pid: u32,
        pub stem: String,
    }

    /// Snapshot all running processes in one pass (cheap, ~1–2 ms)
    pub fn snapshot_processes() -> Vec<ProcEntry> {
        let mut out = Vec::new();
        unsafe {
            let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                Ok(h) => h,
                Err(_) => return out,
            };

            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };

            if Process32FirstW(snap, &mut entry).is_err() {
                let _ = CloseHandle(snap);
                return out;
            }

            loop {
                let raw: Vec<u16> = entry.szExeFile.iter().copied().take_while(|&c| c != 0).collect();
                let name = String::from_utf16_lossy(&raw);
                let stem = std::path::Path::new(&name)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                out.push(ProcEntry {
                    pid: entry.th32ProcessID,
                    parent_pid: entry.th32ParentProcessID,
                    stem,
                });

                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }

            let _ = CloseHandle(snap);
        }
        out
    }

    /// Walk up the process tree to find the closest terminal ancestor PID.
    pub fn find_terminal_ancestor(pid: u32, all: &[ProcEntry]) -> u32 {
        static TERMINAL_STEMS: &[&str] = &[
            "windowsterminal", "wt", "powershell", "pwsh", "cmd", "wt.exe",
            "warp", "code", "iterm2", "hyper", "wezterm-gui",
            "alacritty", "gnome-terminal", "gnome-terminal-server", "conhost",
        ];

        let parent_map: HashMap<u32, u32> = all.iter().map(|e| (e.pid, e.parent_pid)).collect();
        let terminal_pids: HashSet<u32> = all.iter()
            .filter(|e| TERMINAL_STEMS.iter().any(|&t| e.stem == t))
            .map(|e| e.pid)
            .collect();

        let mut current = pid;
        for _ in 0..16 {
            if terminal_pids.contains(&current) {
                return current;
            }
            match parent_map.get(&current) {
                Some(&p) if p != 0 && p != current => current = p,
                _ => break,
            }
        }
        0
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::*;

    pub struct ProcEntry {
        pub pid: u32,
        pub parent_pid: u32,
        pub stem: String,
    }

    pub fn snapshot_processes() -> Vec<ProcEntry> {
        // Read /proc on Linux/macOS
        let mut out = Vec::new();
        if let Ok(dir) = std::fs::read_dir("/proc") {
            for entry in dir.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                let Ok(pid) = name_str.parse::<u32>() else { continue };

                let comm = std::fs::read_to_string(format!("/proc/{}/comm", pid))
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase();

                let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid))
                    .unwrap_or_default();
                let parent_pid = stat.split_whitespace().nth(3)
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);

                out.push(ProcEntry { pid, parent_pid, stem: comm });
            }
        }
        out
    }

    pub fn find_terminal_ancestor(pid: u32, all: &[ProcEntry]) -> u32 {
        static TERMINAL_STEMS: &[&str] = &[
            "bash", "zsh", "fish", "sh", "ksh", "tcsh",
            "gnome-terminal", "konsole", "xterm", "alacritty",
            "wezterm", "iterm2", "hyper", "kitty",
        ];

        let parent_map: std::collections::HashMap<u32, u32> =
            all.iter().map(|e| (e.pid, e.parent_pid)).collect();
        let terminal_pids: std::collections::HashSet<u32> = all.iter()
            .filter(|e| TERMINAL_STEMS.iter().any(|&t| e.stem == t))
            .map(|e| e.pid)
            .collect();

        let mut current = pid;
        for _ in 0..16 {
            if terminal_pids.contains(&current) {
                return current;
            }
            match parent_map.get(&current) {
                Some(&p) if p != 0 && p != current => current = p,
                _ => break,
            }
        }
        0
    }
}

// ── Watcher thread ────────────────────────────────────────────────────────────

pub type SharedSnapshot = Arc<Mutex<Vec<ProcSnapshotEntry>>>;

pub fn new_shared_snapshot() -> SharedSnapshot {
    Arc::new(Mutex::new(Vec::new()))
}

pub fn start_watcher(
    shared: SharedSessions,
    shared_snapshot: SharedSnapshot,
    on_started: impl Fn(AiSessionEvent, &[ProcSnapshotEntry]) + Send + 'static,
    on_stopped: impl Fn(AiSessionEvent) + Send + 'static,
) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(200));

            let procs_internal = platform::snapshot_processes();
            let procs: Vec<ProcSnapshotEntry> = procs_internal
                .iter()
                .map(|e| ProcSnapshotEntry {
                    pid: e.pid,
                    parent_pid: e.parent_pid,
                    stem: e.stem.clone(),
                })
                .collect();
            *shared_snapshot.lock().unwrap() = procs.clone();

            // Build map of currently running AI CLI processes.
            let mut current_ai: HashMap<u32, (String, String)> = HashMap::new();
            for p in &procs {
                if let Some(provider_id) = match_provider(&p.stem) {
                    current_ai.insert(p.pid, (provider_id.to_string(), p.stem.clone()));
                }
            }

            let mut locked = shared.lock().unwrap();
            let prev_pids: HashSet<u32> = locked.keys().copied().collect();
            let curr_pids: HashSet<u32> = current_ai.keys().copied().collect();

            // New processes: insert as unannounced (stability timer starts now).
            for pid in curr_pids.difference(&prev_pids) {
                let (provider_id, exe_name) = current_ai[pid].clone();
                let terminal_pid = platform::find_terminal_ancestor(*pid, &procs_internal);
                let session_id = format!("{}_{}", provider_id, pid);
                println!("[ai-detected] {}.exe pid={} terminal_pid={}", exe_name, pid, terminal_pid);
                locked.insert(*pid, TrackedProcess {
                    session_id,
                    provider_id,
                    terminal_pid,
                    exe_name,
                    first_seen: std::time::Instant::now(),
                    announced: false,
                });
            }

            // Processes gone: fire on_stopped only if they were announced.
            for pid in prev_pids.difference(&curr_pids) {
                if let Some(tracked) = locked.remove(pid) {
                    if tracked.announced {
                        println!("[ai-stopped] {} pid={}", tracked.exe_name, pid);
                        on_stopped(AiSessionEvent {
                            session_id: tracked.session_id,
                            provider_id: tracked.provider_id,
                            pid: *pid,
                            terminal_pid: tracked.terminal_pid,
                            exe_name: tracked.exe_name,
                        });
                    }
                }
            }

            // Announce processes that have passed the stability window.
            let stable_threshold = Duration::from_millis(STABILITY_MS);
            let to_announce: Vec<u32> = locked
                .iter()
                .filter(|(_, t)| !t.announced && t.first_seen.elapsed() >= stable_threshold)
                .map(|(pid, _)| *pid)
                .collect();

            for pid in to_announce {
                if let Some(tracked) = locked.get_mut(&pid) {
                    tracked.announced = true;
                    println!("[ai-stable] {} pid={} → spawning overlay", tracked.exe_name, pid);
                    on_started(
                        AiSessionEvent {
                            session_id: tracked.session_id.clone(),
                            provider_id: tracked.provider_id.clone(),
                            pid,
                            terminal_pid: tracked.terminal_pid,
                            exe_name: tracked.exe_name.clone(),
                        },
                        &procs,
                    );
                }
            }
        }
    });
}

/// Snapshot currently active (stable, announced) sessions.
pub fn get_active_sessions(shared: &SharedSessions) -> ActiveSessions {
    let locked = shared.lock().unwrap();
    let sessions = locked
        .iter()
        .filter(|(_, t)| t.announced)
        .map(|(pid, t)| AiSessionEvent {
            session_id: t.session_id.clone(),
            provider_id: t.provider_id.clone(),
            pid: *pid,
            terminal_pid: t.terminal_pid,
            exe_name: t.exe_name.clone(),
        })
        .collect();
    ActiveSessions { sessions }
}
