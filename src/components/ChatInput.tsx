import { useState, type KeyboardEvent } from "react";
import type { Agent } from "../types/agent";
import type { SessionMode } from "../types/session";

interface ChatInputProps {
  mode: SessionMode;
  running: boolean;
  roster: Agent[];
  onModeChange: (mode: SessionMode) => void;
  onSend: (text: string) => void;
}

const modeButtonClass = (active: boolean) =>
  "rounded-md px-3 py-1.5 text-sm font-medium transition-colors " +
  (active ? "bg-indigo-600 text-white" : "border border-slate-300 text-slate-600 hover:bg-slate-100");

/**
 * Message box with the two mode buttons inside it. "小组发言" (sequential)
 * is the default; "点名发言" (mention) expands the session's agents so one
 * can be picked, and clicking "小组发言" again switches back. Mention mode's
 * turn-taking isn't implemented yet, so sending is disabled while it's on.
 */
export function ChatInput({ mode, running, roster, onModeChange, onSend }: ChatInputProps) {
  const [text, setText] = useState("");
  const [mentionTargetId, setMentionTargetId] = useState<string | null>(null);

  const mentionMode = mode === "mention";
  const blockedReason = running
    ? "本轮进行中，请等待所有 Agent 发言完毕"
    : roster.length === 0
      ? "名单里还没有 Agent，先在上方“管理名单”里添加"
      : mentionMode
        ? "点名发言的对话逻辑尚未实现，请切换回小组发言"
        : null;
  const canSend = blockedReason === null && text.trim() !== "";

  function send() {
    if (!canSend) return;
    onSend(text.trim());
    setText("");
  }

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    // Enter while a Chinese/Japanese IME is composing only confirms the
    // candidate, it must not send the message.
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      send();
    }
  }

  return (
    <div className="rounded-lg border border-slate-300 bg-white p-3 shadow-sm focus-within:border-indigo-500 space-y-2">
      {mentionMode && roster.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-xs text-slate-500">点名对象</span>
          {roster.map((agent) => (
            <button
              key={agent.id}
              type="button"
              onClick={() => setMentionTargetId(agent.id)}
              className={
                "flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs " +
                (mentionTargetId === agent.id
                  ? "border-indigo-500 bg-indigo-50 text-indigo-700"
                  : "border-slate-300 text-slate-600 hover:bg-slate-50")
              }
            >
              <span
                className="h-2 w-2 rounded-full"
                style={{ backgroundColor: agent.color ?? "#94a3b8" }}
                aria-hidden
              />
              {agent.name}
            </button>
          ))}
        </div>
      )}

      <textarea
        rows={3}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="输入消息，Enter 发送，Shift+Enter 换行"
        className="w-full resize-none border-0 p-0 text-sm text-slate-900 focus:outline-none focus:ring-0"
      />

      <div className="flex items-center justify-between gap-3">
        <p className="text-xs text-slate-400">{blockedReason}</p>
        <div className="flex shrink-0 items-center gap-2">
          <button
            type="button"
            onClick={() => onModeChange("sequential")}
            className={modeButtonClass(!mentionMode)}
          >
            小组发言
          </button>
          <button
            type="button"
            onClick={() => onModeChange("mention")}
            className={modeButtonClass(mentionMode)}
          >
            点名发言
          </button>
          <button
            type="button"
            onClick={send}
            disabled={!canSend}
            className="rounded-md bg-indigo-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-40"
          >
            {running ? "发言中..." : "发送"}
          </button>
        </div>
      </div>
    </div>
  );
}
