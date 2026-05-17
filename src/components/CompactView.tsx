import { motion } from "framer-motion";
import type { HudState } from "../lib/types";
import { formatTokens } from "../lib/format";
import { getProviderMeta } from "../lib/constants";

interface Props {
  state: HudState;
  onExpand: () => void;
}

export default function CompactView({ state, onExpand }: Props) {
  const activeProviders = state.providers.filter((p) => p.is_running);
  const hasActive = activeProviders.length > 0;

  return (
    <div
      className="hud-root flex items-center justify-end w-full h-full pr-4"
      onDoubleClick={onExpand}
    >
      <motion.div
        initial={{ scale: 0.9, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        className="compact-pill"
        style={{
          borderColor: hasActive ? "rgba(192, 132, 252, 0.4)" : "rgba(255,255,255,0.1)",
          boxShadow: hasActive
            ? "0 0 12px rgba(192, 132, 252, 0.3), 0 0 30px rgba(192, 132, 252, 0.1)"
            : "none",
        }}
      >
        {/* Status dot */}
        <span className={`w-2 h-2 rounded-full shrink-0 ${hasActive ? "pulse-dot bg-green-400" : "bg-white/20"}`}
          style={hasActive ? { boxShadow: "0 0 6px rgba(74, 222, 128, 0.5)" } : {}} />

        {/* Title */}
        <span className="compact-title">AI-HUD</span>

        {/* Active provider icons */}
        {activeProviders.length > 0 && (
          <div className="compact-providers">
            {activeProviders.slice(0, 4).map((p) => {
              const meta = getProviderMeta(p.id);
              return (
                <span
                  key={p.id}
                  className="compact-provider-dot"
                  style={{ color: meta.color, textShadow: `0 0 6px ${meta.glowColor}` }}
                  title={meta.label}
                >
                  {meta.icon}
                </span>
              );
            })}
            {activeProviders.length > 4 && (
              <span className="compact-more">+{activeProviders.length - 4}</span>
            )}
          </div>
        )}

        {/* Token count */}
        {hasActive && (
          <span className="compact-tokens">
            {formatTokens(state.total_session_tokens)}
          </span>
        )}
      </motion.div>
    </div>
  );
}
