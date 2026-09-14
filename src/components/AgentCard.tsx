import type { Agent } from "../types/agent";

interface AgentCardProps {
  agent: Agent;
  onEdit: (agent: Agent) => void;
  onDelete: (agent: Agent) => void;
}

export function AgentCard({ agent, onEdit, onDelete }: AgentCardProps) {
  return (
    <div className="flex items-center justify-between rounded-lg border border-slate-200 bg-white p-3 shadow-sm">
      <div className="flex items-center gap-3">
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

      <div className="flex items-center gap-2">
        <button
          onClick={() => onEdit(agent)}
          className="rounded-md px-3 py-1 text-xs font-medium text-indigo-600 hover:bg-indigo-50"
        >
          Edit
        </button>
        <button
          onClick={() => onDelete(agent)}
          className="rounded-md px-3 py-1 text-xs font-medium text-red-600 hover:bg-red-50"
        >
          Delete
        </button>
      </div>
    </div>
  );
}
