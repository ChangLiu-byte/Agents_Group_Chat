import { invoke } from "@tauri-apps/api/core";
import type { SessionMode } from "../types/session";

interface ModeToggleProps {
  sessionId: string;
  mode: SessionMode;
  onModeChange: (mode: SessionMode) => void;
}

const baseButtonClass =
  "rounded-md px-3 py-1.5 text-sm font-medium transition-colors disabled:opacity-50";

/**
 * "小组发言" (sequential) / "点名发言" (mention) switch. Clicking the
 * inactive side calls `set_session_mode` and updates local state - mention
 * mode's actual turn-taking logic isn't implemented yet, this just persists
 * the flag so the rest of the UI (and later, the chat input's per-agent
 * picker) can be built against it now.
 */
export function ModeToggle({ sessionId, mode, onModeChange }: ModeToggleProps) {
  async function switchTo(next: SessionMode) {
    if (next === mode) return;
    await invoke("set_session_mode", { sessionId, mode: next });
    onModeChange(next);
  }

  return (
    <div className="inline-flex rounded-md border border-slate-300 bg-slate-100 p-1">
      <button
        type="button"
        onClick={() => switchTo("sequential")}
        className={
          baseButtonClass +
          " " +
          (mode === "sequential"
            ? "bg-white text-indigo-600 shadow-sm"
            : "text-slate-600 hover:text-slate-900")
        }
      >
        小组发言
      </button>
      <button
        type="button"
        onClick={() => switchTo("mention")}
        className={
          baseButtonClass +
          " " +
          (mode === "mention"
            ? "bg-white text-indigo-600 shadow-sm"
            : "text-slate-600 hover:text-slate-900")
        }
      >
        点名发言
      </button>
    </div>
  );
}
