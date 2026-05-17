/// Per-window session identity, parsed from the URL query the Rust
/// OverlayManager passed at window creation time.
///
/// The "main" controller window has no session params and returns null.
/// Per-session overlay windows return their session details.

export interface SessionContext {
  sessionId: string;
  providerId: string;
  terminalPid: number;
  aiPid: number;
}

export function getCurrentSession(): SessionContext | null {
  if (typeof window === "undefined") return null;
  const params = new URLSearchParams(window.location.search);
  const sessionId = params.get("session_id");
  if (!sessionId) return null;
  return {
    sessionId,
    providerId: params.get("provider_id") ?? "",
    terminalPid: Number(params.get("terminal_pid") ?? 0),
    aiPid: Number(params.get("ai_pid") ?? 0),
  };
}
