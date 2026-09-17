/** Keep in sync with the `Agent` struct in `src-tauri/src/agent.rs`. */
export interface Agent {
  id: string;
  name: string;
  provider: string;
  model: string;
  system_prompt: string | null;
  temperature: number;
  color: string | null;
  /** Unix timestamp (seconds since epoch). */
  created_at: number;
}

export const PROVIDERS = ["openai", "anthropic", "deepseek", "qwen"] as const;
export type Provider = (typeof PROVIDERS)[number];

/** Shape of the form while the user is editing it (before it becomes an `Agent`). */
export interface AgentFormValues {
  id: string;
  name: string;
  provider: string;
  model: string;
  system_prompt: string;
  temperature: number;
  color: string;
  api_key: string;
}

export const DEFAULT_COLOR = "#6366f1";

export function emptyFormValues(): AgentFormValues {
  return {
    id: "",
    name: "",
    provider: PROVIDERS[0],
    model: "",
    system_prompt: "",
    temperature: 0.7,
    color: DEFAULT_COLOR,
    api_key: "",
  };
}

export function agentToFormValues(agent: Agent): AgentFormValues {
  return {
    id: agent.id,
    name: agent.name,
    provider: agent.provider,
    model: agent.model,
    system_prompt: agent.system_prompt ?? "",
    temperature: agent.temperature,
    color: agent.color ?? DEFAULT_COLOR,
    api_key: "",
  };
}
