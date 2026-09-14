import { useState, type FormEvent } from "react";
import { PROVIDERS, type AgentFormValues } from "../types/agent";

interface AgentFormProps {
  initialValues: AgentFormValues;
  isEditing: boolean;
  onSubmit: (values: AgentFormValues) => void | Promise<void>;
  onCancel: () => void;
}

const inputClass =
  "w-full rounded-md border border-slate-300 px-3 py-1.5 text-sm text-slate-900 " +
  "focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500";

const labelClass = "block text-sm font-medium text-slate-700 mb-1";

export function AgentForm({
  initialValues,
  isEditing,
  onSubmit,
  onCancel,
}: AgentFormProps) {
  const [values, setValues] = useState<AgentFormValues>(initialValues);
  const [submitting, setSubmitting] = useState(false);

  function update<K extends keyof AgentFormValues>(key: K, value: AgentFormValues[K]) {
    setValues((prev) => ({ ...prev, [key]: value }));
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    try {
      await onSubmit(values);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="rounded-lg border border-slate-200 bg-white p-4 shadow-sm space-y-4"
    >
      <h2 className="text-base font-semibold text-slate-900">
        {isEditing ? "编辑 Agent" : "新增 Agent"}
      </h2>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        <div>
          <label className={labelClass} htmlFor="agent-name">
            名称
          </label>
          <input
            id="agent-name"
            className={inputClass}
            value={values.name}
            onChange={(e) => update("name", e.target.value)}
            required
          />
        </div>

        <div>
          <label className={labelClass} htmlFor="agent-provider">
            Provider
          </label>
          <select
            id="agent-provider"
            className={inputClass}
            value={values.provider}
            onChange={(e) => update("provider", e.target.value)}
          >
            {PROVIDERS.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className={labelClass} htmlFor="agent-model">
            Model
          </label>
          <input
            id="agent-model"
            className={inputClass}
            placeholder="e.g. gpt-4o"
            value={values.model}
            onChange={(e) => update("model", e.target.value)}
            required
          />
        </div>

        <div>
          <label className={labelClass} htmlFor="agent-temperature">
            Temperature
          </label>
          <input
            id="agent-temperature"
            type="number"
            step="0.1"
            min="0"
            max="2"
            className={inputClass}
            value={values.temperature}
            onChange={(e) => update("temperature", Number(e.target.value))}
          />
        </div>

        <div>
          <label className={labelClass} htmlFor="agent-color">
            颜色
          </label>
          <div className="flex items-center gap-2">
            <input
              id="agent-color"
              type="color"
              className="h-9 w-10 shrink-0 cursor-pointer rounded border border-slate-300"
              value={values.color}
              onChange={(e) => update("color", e.target.value)}
            />
            <input
              type="text"
              className={inputClass}
              value={values.color}
              onChange={(e) => update("color", e.target.value)}
            />
          </div>
        </div>

        <div>
          <label className={labelClass} htmlFor="agent-api-key">
            API Key
          </label>
          <input
            id="agent-api-key"
            type="password"
            autoComplete="off"
            className={inputClass}
            placeholder={isEditing ? "留空以保留原密钥" : "粘贴你的 API key"}
            value={values.api_key}
            onChange={(e) => update("api_key", e.target.value)}
            required={!isEditing}
          />
        </div>
      </div>

      <div>
        <label className={labelClass} htmlFor="agent-system-prompt">
          System Prompt
        </label>
        <textarea
          id="agent-system-prompt"
          rows={3}
          className={inputClass}
          value={values.system_prompt}
          onChange={(e) => update("system_prompt", e.target.value)}
        />
      </div>

      <div className="flex items-center gap-2 pt-1">
        <button
          type="submit"
          disabled={submitting}
          className="rounded-md bg-indigo-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
        >
          {submitting ? "保存中..." : isEditing ? "保存" : "添加"}
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
