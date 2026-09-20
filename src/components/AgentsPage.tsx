import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AgentForm } from "./AgentForm";
import { AgentCard } from "./AgentCard";
import { TestPanel } from "./TestPanel";
import {
  agentToFormValues,
  emptyFormValues,
  type Agent,
  type AgentFormValues,
} from "../types/agent";

type FormMode = { kind: "closed" } | { kind: "add" } | { kind: "edit"; agent: Agent };

/** The original Phase 2/3 "Agent 配置" page, unchanged, just moved out of
 * `App.tsx` now that the app has more than one page (see `SessionsPage`). */
export function AgentsPage() {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [formMode, setFormMode] = useState<FormMode>({ kind: "closed" });
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    try {
      const result = await invoke<Agent[]>("list_agents");
      setAgents(result);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleSubmit(values: AgentFormValues) {
    try {
      if (formMode.kind === "edit") {
        const agent: Agent = {
          id: values.id,
          name: values.name,
          provider: values.provider,
          model: values.model,
          system_prompt: values.system_prompt || null,
          temperature: values.temperature,
          color: values.color || null,
          created_at: formMode.agent.created_at,
        };
        await invoke("update_agent", {
          agent,
          apiKey: values.api_key ? values.api_key : null,
        });
      } else {
        const agent: Agent = {
          id: crypto.randomUUID(),
          name: values.name,
          provider: values.provider,
          model: values.model,
          system_prompt: values.system_prompt || null,
          temperature: values.temperature,
          color: values.color || null,
          created_at: Math.floor(Date.now() / 1000),
        };
        await invoke("add_agent", { agent, apiKey: values.api_key });
      }
      setFormMode({ kind: "closed" });
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDelete(agent: Agent) {
    if (!confirm(`删除 Agent "${agent.name}"？此操作无法撤销。`)) return;
    try {
      await invoke("delete_agent", { id: agent.id });
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="space-y-6">
      <header className="flex items-center justify-between">
        <h1 className="text-xl font-bold text-slate-900">Agent 配置</h1>
        {formMode.kind === "closed" && (
          <button
            onClick={() => setFormMode({ kind: "add" })}
            className="rounded-md bg-indigo-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-indigo-500"
          >
            + 新增 Agent
          </button>
        )}
      </header>

      {error && (
        <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
          {error}
        </div>
      )}

      {formMode.kind !== "closed" && (
        <AgentForm
          key={formMode.kind === "edit" ? formMode.agent.id : "new"}
          initialValues={
            formMode.kind === "edit" ? agentToFormValues(formMode.agent) : emptyFormValues()
          }
          isEditing={formMode.kind === "edit"}
          onSubmit={handleSubmit}
          onCancel={() => setFormMode({ kind: "closed" })}
        />
      )}

      <section className="space-y-2">
        {agents.length === 0 && (
          <p className="text-sm text-slate-500">还没有配置任何 Agent。</p>
        )}
        {agents.map((agent) => (
          <AgentCard
            key={agent.id}
            agent={agent}
            onEdit={(a) => setFormMode({ kind: "edit", agent: a })}
            onDelete={handleDelete}
          />
        ))}
      </section>

      <TestPanel agents={agents} />
    </div>
  );
}
