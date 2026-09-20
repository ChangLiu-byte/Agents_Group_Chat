import { invoke } from "@tauri-apps/api/core";
import type { Agent } from "../types/agent";

interface SessionAgentRosterProps {
  sessionId: string;
  roster: Agent[];
  allAgents: Agent[];
  onRosterChange: (roster: Agent[]) => void;
  onError: (message: string) => void;
}

/**
 * The session's speaking order: reorder via up/down arrows (no
 * drag-and-drop for now, per plan), remove, and add agents not yet on the
 * roster. Always sends the full new order to `reorder_session_agents`
 * rather than an incremental move, matching that command's contract.
 */
export function SessionAgentRoster({
  sessionId,
  roster,
  allAgents,
  onRosterChange,
  onError,
}: SessionAgentRosterProps) {
  const rosterIds = new Set(roster.map((a) => a.id));
  const availableAgents = allAgents.filter((a) => !rosterIds.has(a.id));

  async function move(index: number, direction: -1 | 1) {
    const target = index + direction;
    if (target < 0 || target >= roster.length) return;

    const reordered = [...roster];
    [reordered[index], reordered[target]] = [reordered[target], reordered[index]];

    try {
      await invoke("reorder_session_agents", {
        sessionId,
        orderedAgentIds: reordered.map((a) => a.id),
      });
      onRosterChange(reordered);
    } catch (e) {
      onError(String(e));
    }
  }

  async function remove(agent: Agent) {
    try {
      await invoke("remove_agent_from_session", { sessionId, agentId: agent.id });
      onRosterChange(roster.filter((a) => a.id !== agent.id));
    } catch (e) {
      onError(String(e));
    }
  }

  async function add(agent: Agent) {
    try {
      await invoke("add_agent_to_session", { sessionId, agentId: agent.id });
      onRosterChange([...roster, agent]);
    } catch (e) {
      onError(String(e));
    }
  }

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        {roster.length === 0 && (
          <p className="text-sm text-slate-500">还没有添加任何 Agent，从下面选择加入名单。</p>
        )}
        {roster.map((agent, index) => (
          <div
            key={agent.id}
            className="flex items-center justify-between rounded-lg border border-slate-200 bg-white p-3 shadow-sm"
          >
            <div className="flex items-center gap-3">
              <span className="w-5 text-center text-xs font-medium text-slate-400">
                {index + 1}
              </span>
              <span
                className="h-3 w-3 shrink-0 rounded-full border border-black/10"
                style={{ backgroundColor: agent.color ?? "#94a3b8" }}
                aria-hidden
              />
              <div>
                <p className="text-sm font-semibold text-slate-900">{agent.name}</p>
                <p className="text-xs text-slate-500">
                  {agent.provider} · {agent.model}
                </p>
              </div>
            </div>

            <div className="flex items-center gap-1">
              <button
                type="button"
                onClick={() => move(index, -1)}
                disabled={index === 0}
                className="rounded-md px-2 py-1 text-slate-500 hover:bg-slate-100 disabled:opacity-30"
                aria-label="上移"
              >
                ↑
              </button>
              <button
                type="button"
                onClick={() => move(index, 1)}
                disabled={index === roster.length - 1}
                className="rounded-md px-2 py-1 text-slate-500 hover:bg-slate-100 disabled:opacity-30"
                aria-label="下移"
              >
                ↓
              </button>
              <button
                type="button"
                onClick={() => remove(agent)}
                className="rounded-md px-3 py-1 text-xs font-medium text-red-600 hover:bg-red-50"
              >
                移除
              </button>
            </div>
          </div>
        ))}
      </div>

      {availableAgents.length > 0 && (
        <div className="space-y-2">
          <p className="text-xs font-medium text-slate-500">添加 Agent 到名单</p>
          <div className="flex flex-wrap gap-2">
            {availableAgents.map((agent) => (
              <button
                key={agent.id}
                type="button"
                onClick={() => add(agent)}
                className="rounded-md border border-slate-300 bg-white px-3 py-1.5 text-sm text-slate-700 hover:border-indigo-400 hover:text-indigo-600"
              >
                + {agent.name}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
