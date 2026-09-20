import { useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Agent } from "../types/agent";
import type { Session } from "../types/session";

interface NewSessionFormProps {
  allAgents: Agent[];
  onCreated: (session: Session, selectedAgentIds: string[]) => void;
  onCancel: () => void;
}

/** Topic input + agent picker, in one screen. On submit: create the
 * session, then add each checked agent to its roster in order. */
export function NewSessionForm({ allAgents, onCreated, onCancel }: NewSessionFormProps) {
  const [topic, setTopic] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function toggle(agentId: string) {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(agentId)) {
        next.delete(agentId);
      } else {
        next.add(agentId);
      }
      return next;
    });
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      const session = await invoke<Session>("create_session", { topic });
      const orderedIds = allAgents.map((a) => a.id).filter((id) => selectedIds.has(id));
      for (const agentId of orderedIds) {
        await invoke("add_agent_to_session", { sessionId: session.id, agentId });
      }
      onCreated(session, orderedIds);
    } catch (err) {
      setError(String(err));
      setSubmitting(false);
    }
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="rounded-lg border border-slate-200 bg-white p-4 shadow-sm space-y-4"
    >
      <h2 className="text-base font-semibold text-slate-900">新建会话</h2>

      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
          {error}
        </div>
      )}

      <div>
        <label className="block text-sm font-medium text-slate-700 mb-1" htmlFor="session-topic">
          话题
        </label>
        <input
          id="session-topic"
          className="w-full rounded-md border border-slate-300 px-3 py-1.5 text-sm text-slate-900 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500"
          value={topic}
          onChange={(e) => setTopic(e.target.value)}
          placeholder="例如：下一版本要不要做暗色模式？"
          required
        />
      </div>

      <div>
        <p className="block text-sm font-medium text-slate-700 mb-2">选择参会 Agent</p>
        {allAgents.length === 0 && (
          <p className="text-sm text-slate-500">还没有配置任何 Agent，先去"Agent 配置"页添加。</p>
        )}
        <div className="space-y-1">
          {allAgents.map((agent) => (
            <label
              key={agent.id}
              className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-slate-800 hover:bg-slate-50"
            >
              <input
                type="checkbox"
                checked={selectedIds.has(agent.id)}
                onChange={() => toggle(agent.id)}
                className="rounded border-slate-300 text-indigo-600 focus:ring-indigo-500"
              />
              <span
                className="h-3 w-3 shrink-0 rounded-full border border-black/10"
                style={{ backgroundColor: agent.color ?? "#94a3b8" }}
                aria-hidden
              />
              {agent.name}
              <span className="text-xs text-slate-400">
                {agent.provider} · {agent.model}
              </span>
            </label>
          ))}
        </div>
      </div>

      <div className="flex items-center gap-2 pt-1">
        <button
          type="submit"
          disabled={submitting}
          className="rounded-md bg-indigo-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
        >
          {submitting ? "创建中..." : "创建会话"}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-4 py-1.5 text-sm font-medium text-slate-600 hover:bg-slate-100"
        >
          取消
        </button>
      </div>
    </form>
  );
}
