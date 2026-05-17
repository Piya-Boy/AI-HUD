import { useState, useCallback } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useProviders } from "./hooks/useProviders";
import { useInteractiveHold } from "./hooks/useInteractiveHold";
import { getCurrentSession } from "./lib/session";
import ExpandedView from "./components/ExpandedView";
import CompactView from "./components/CompactView";

export default function App() {
  const session = getCurrentSession();
  const { state, refresh } = useProviders();
  const [compact, setCompact] = useState(false);

  // Hold ALT to temporarily disable click-through and interact with the overlay
  useInteractiveHold();

  const handleMinimize = useCallback(() => setCompact(true), []);
  const handleExpand = useCallback(() => setCompact(false), []);

  // The main controller window has no session — render nothing (it's hidden anyway).
  // Position/lifecycle for session overlays is fully driven by the Rust OverlayManager.
  if (!session) {
    return null;
  }

  if (!state) {
    return (
      <div style={{ display: "flex", alignItems: "center", justifyContent: "center", width: "100%", height: "100%" }}>
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          className="loading-state"
        >
          <div className="loading-spinner" />
          <span className="loading-text">Loading {session.providerId}...</span>
        </motion.div>
      </div>
    );
  }

  return (
    <AnimatePresence mode="wait">
      {compact ? (
        <motion.div
          key="compact"
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          exit={{ opacity: 0, scale: 0.95 }}
          transition={{ duration: 0.15 }}
          style={{ width: "100%", height: "100%" }}
        >
          <CompactView
            state={state}
            onExpand={handleExpand}
          />
        </motion.div>
      ) : (
        <motion.div
          key="expanded"
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          exit={{ opacity: 0, scale: 0.95 }}
          transition={{ duration: 0.15 }}
          style={{ width: "100%", height: "100%" }}
        >
          <ExpandedView
            state={state}
            onMinimize={handleMinimize}
            onRefresh={refresh}
          />
        </motion.div>
      )}
    </AnimatePresence>
  );
}
