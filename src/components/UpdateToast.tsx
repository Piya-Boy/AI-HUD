import { motion, AnimatePresence } from "framer-motion";
import { useUpdater } from "../hooks/useUpdater";

export default function UpdateToast() {
  const { status, showBanner, skipVersion, dismiss } = useUpdater();

  if (!showBanner || !status?.latest_version) return null;

  return (
    <AnimatePresence>
      <motion.div
        key="update-toast"
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -8 }}
        transition={{ duration: 0.18 }}
        className="update-toast"
      >
        <span className="update-toast-text">
          v{status.latest_version} available
        </span>
        <div className="update-toast-actions">
          <button
            className="update-toast-btn update-toast-btn--primary"
            onClick={() => {
              // Open GitHub releases page — user downloads manually.
              // When a signing key + endpoint are configured, this can
              // trigger tauri_plugin_updater install instead.
              open("https://github.com/Piya-Boy/AI-HUD/releases/latest");
              dismiss();
            }}
          >
            Update
          </button>
          <button
            className="update-toast-btn"
            onClick={() => skipVersion(status.latest_version!)}
          >
            Skip
          </button>
          <button
            className="update-toast-btn"
            onClick={dismiss}
          >
            ✕
          </button>
        </div>
      </motion.div>
    </AnimatePresence>
  );
}
