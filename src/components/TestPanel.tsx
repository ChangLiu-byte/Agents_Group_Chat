import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Agent } from "../types/agent";

interface TestPanelProps {
  agents: Agent[];
}

/**
 * TEMPORARY dev-only panel for manually exercising the Phase 3 provider
 * adapter layer (`test_agent_message`) before the real chat UI exists.
 * Not part of the actual product - safe to delete once Phase 4 (real
 * chat view with history) lands.
 */
export function TestPanel({ agents }: TestPanelProps) {
  const [agentId, setAgentId] = useState("");
  const [message, setMessage] = useState("你好，你是谁？");
  const [reply, setReply] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function handleSend() {
    if (!agentId) return;
    setLoading(true);
    setReply(null);
    setError(null);
    try {
      const result = await invoke<string>("test_agent_message", {
        agentId,
        userMessage: message,
      });
      setReply(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section className="rounded-lg border border-dashed border-amber-400 bg-amber-50 p-4 space-y-3">
      <h2 className="text-sm font-semibold text-amber-800">
        [临时测试面板] Provider Adapter 手测 · 会在 Phase 4 移除
      </h2>

      <div className="flex flex-col sm:flex-row gap-2">
        <select
          className="rounded-md border border-slate-300 bg-white px-3 py-1.5 text-sm text-slate-900"
          value={agentId}
          onChange={(e) => setAgentId(e.target.value)}
        >
          <option value="">选择一个 agent...</option>
          {agents.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name} ({a.provider} / {a.model})
            </option>
          ))}
        </select>
        <input
          className="flex-1 rounded-md border border-slate-300 px-3 py-1.5 text-sm text-slate-900"
          value={message}
          onChange={(e) => setMessage(e.target.value)}
        />
        <button
          onClick={handleSend}
          disabled={!agentId || loading}
          className="rounded-md bg-amber-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-amber-500 disabled:opacity-50"
        >
          {loading ? "发送中..." : "发送"}
        </button>
      </div>

      {error && (
        <p className="whitespace-pre-wrap rounded-md bg-red-50 px-3 py-2 text-xs text-red-700">
          {error}
        </p>
      )}
      {reply && (
        <p className="whitespace-pre-wrap rounded-md bg-white px-3 py-2 text-sm text-slate-800">
          {reply}
        </p>
      )}
    </section>
  );
}
