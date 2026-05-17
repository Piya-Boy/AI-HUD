export interface ProviderMeta {
  id: string;
  label: string;
  icon: string;
  color: string;
  glowColor: string;
  gradient: string;
}

export const PROVIDER_META: Record<string, ProviderMeta> = {
  claude: {
    id: "claude",
    label: "Claude",
    icon: "⟡",
    color: "#c084fc",
    glowColor: "rgba(192, 132, 252, 0.6)",
    gradient: "linear-gradient(135deg, #c084fc, #818cf8)",
  },
  gemini: {
    id: "gemini",
    label: "Gemini",
    icon: "◆",
    color: "#60a5fa",
    glowColor: "rgba(96, 165, 250, 0.6)",
    gradient: "linear-gradient(135deg, #60a5fa, #34d399)",
  },
  aider: {
    id: "aider",
    label: "Aider",
    icon: "▲",
    color: "#34d399",
    glowColor: "rgba(52, 211, 153, 0.6)",
    gradient: "linear-gradient(135deg, #34d399, #a3e635)",
  },
  ollama: {
    id: "ollama",
    label: "Ollama",
    icon: "●",
    color: "#f472b6",
    glowColor: "rgba(244, 114, 182, 0.6)",
    gradient: "linear-gradient(135deg, #f472b6, #c084fc)",
  },
  openai: {
    id: "openai",
    label: "Codex",
    icon: "◈",
    color: "#4ade80",
    glowColor: "rgba(74, 222, 128, 0.6)",
    gradient: "linear-gradient(135deg, #4ade80, #22d3ee)",
  },
  openrouter: {
    id: "openrouter",
    label: "OpenRouter",
    icon: "⬡",
    color: "#fb923c",
    glowColor: "rgba(251, 146, 60, 0.6)",
    gradient: "linear-gradient(135deg, #fb923c, #f87171)",
  },
  continue: {
    id: "continue",
    label: "Continue",
    icon: "▶",
    color: "#22d3ee",
    glowColor: "rgba(34, 211, 238, 0.6)",
    gradient: "linear-gradient(135deg, #22d3ee, #818cf8)",
  },
  cursor: {
    id: "cursor",
    label: "Cursor",
    icon: "◉",
    color: "#a78bfa",
    glowColor: "rgba(167, 139, 250, 0.6)",
    gradient: "linear-gradient(135deg, #a78bfa, #f472b6)",
  },
};

export const DEFAULT_PROVIDER_META: ProviderMeta = {
  id: "unknown",
  label: "Custom",
  icon: "◎",
  color: "#94a3b8",
  glowColor: "rgba(148, 163, 184, 0.6)",
  gradient: "linear-gradient(135deg, #94a3b8, #64748b)",
};

export function getProviderMeta(id: string): ProviderMeta {
  return PROVIDER_META[id] ?? { ...DEFAULT_PROVIDER_META, id, label: id };
}

export const ACTIVITY_LABELS: Record<string, string> = {
  Idle: "IDLE",
  Thinking: "THINKING",
  Coding: "CODING",
  Generating: "GENERATING",
  Refactoring: "REFACTORING",
  Musing: "MUSING",
  Analyzing: "ANALYZING",
  Searching: "SEARCHING",
  Unknown: "WORKING",
};
